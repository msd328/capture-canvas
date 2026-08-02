use super::{OidcCallbackStatus, CALLBACK_TIMEOUT};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const AUTHORIZATION_CODE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallbackStage {
    Idle,
    Waiting,
    CodeReceived,
    ProviderError,
    TimedOut,
    Cancelled,
    Failed,
}

impl CallbackStage {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Waiting => "waiting",
            Self::CodeReceived => "codeReceived",
            Self::ProviderError => "providerError",
            Self::TimedOut => "timedOut",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

struct SecretText(Vec<u8>);

impl SecretText {
    fn new(value: String) -> Self {
        Self(value.into_bytes())
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

struct ActiveCallback {
    generation: u64,
    expected_state: SecretText,
    expires_at_instant: Instant,
}

struct NativeAuthorizationGrant {
    _generation: u64,
    _state: SecretText,
    _authorization_code: SecretText,
    expires_at_instant: Instant,
}

struct CallbackState {
    generation: u64,
    stage: CallbackStage,
    active: Option<ActiveCallback>,
    grant: Option<NativeAuthorizationGrant>,
    last_expires_at: Option<DateTime<Utc>>,
}

impl Default for CallbackState {
    fn default() -> Self {
        Self {
            generation: 0,
            stage: CallbackStage::Idle,
            active: None,
            grant: None,
            last_expires_at: None,
        }
    }
}

struct OidcCallbackRuntime {
    state: Mutex<CallbackState>,
}

impl OidcCallbackRuntime {
    fn new() -> Self {
        Self {
            state: Mutex::new(CallbackState::default()),
        }
    }

    fn begin(&self, expected_state: String, expires_at: DateTime<Utc>) -> u64 {
        let mut state = self.state.lock();
        state.generation = state.generation.saturating_add(1);
        let generation = state.generation;
        state.stage = CallbackStage::Waiting;
        state.grant = None;
        state.last_expires_at = Some(expires_at);
        state.active = Some(ActiveCallback {
            generation,
            expected_state: SecretText::new(expected_state),
            expires_at_instant: Instant::now() + CALLBACK_TIMEOUT,
        });
        generation
    }

    fn is_active(&self, generation: u64) -> bool {
        let mut state = self.state.lock();
        Self::expire_locked(&mut state);
        state.stage == CallbackStage::Waiting
            && state
                .active
                .as_ref()
                .is_some_and(|active| active.generation == generation)
    }

    fn accept_code(
        &self,
        generation: u64,
        supplied_state: String,
        authorization_code: String,
    ) -> Result<(), AcceptError> {
        let mut state = self.state.lock();
        Self::expire_locked(&mut state);
        let Some(active) = state.active.as_ref() else {
            return Err(AcceptError::NotActive);
        };
        if active.generation != generation || state.stage != CallbackStage::Waiting {
            return Err(AcceptError::NotActive);
        }
        if !constant_time_eq(active.expected_state.as_bytes(), supplied_state.as_bytes()) {
            return Err(AcceptError::StateMismatch);
        }

        state.active = None;
        state.stage = CallbackStage::CodeReceived;
        state.grant = Some(NativeAuthorizationGrant {
            _generation: generation,
            _state: SecretText::new(supplied_state),
            _authorization_code: SecretText::new(authorization_code),
            expires_at_instant: Instant::now() + AUTHORIZATION_CODE_TTL,
        });
        Ok(())
    }

    fn accept_provider_error(
        &self,
        generation: u64,
        supplied_state: String,
    ) -> Result<(), AcceptError> {
        let mut state = self.state.lock();
        Self::expire_locked(&mut state);
        let Some(active) = state.active.as_ref() else {
            return Err(AcceptError::NotActive);
        };
        if active.generation != generation || state.stage != CallbackStage::Waiting {
            return Err(AcceptError::NotActive);
        }
        if !constant_time_eq(active.expected_state.as_bytes(), supplied_state.as_bytes()) {
            return Err(AcceptError::StateMismatch);
        }

        state.active = None;
        state.grant = None;
        state.stage = CallbackStage::ProviderError;
        Ok(())
    }

    fn timeout(&self, generation: u64) {
        let mut state = self.state.lock();
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            state.active = None;
            state.grant = None;
            state.stage = CallbackStage::TimedOut;
        }
    }

    fn fail(&self, generation: u64) {
        let mut state = self.state.lock();
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            state.active = None;
            state.grant = None;
            state.stage = CallbackStage::Failed;
        }
    }

    fn cancel(&self) -> bool {
        let mut state = self.state.lock();
        let had_pending = state.active.is_some() || state.grant.is_some();
        state.generation = state.generation.saturating_add(1);
        state.active = None;
        state.grant = None;
        if had_pending {
            state.stage = CallbackStage::Cancelled;
        }
        had_pending
    }

    fn status(&self) -> OidcCallbackStatus {
        let mut state = self.state.lock();
        Self::expire_locked(&mut state);
        let pending = state.stage == CallbackStage::Waiting;
        OidcCallbackStatus {
            stage: state.stage.label(),
            pending,
            code_received: state.stage == CallbackStage::CodeReceived,
            provider_error: state.stage == CallbackStage::ProviderError,
            expires_at: if pending {
                state.last_expires_at.clone()
            } else {
                None
            },
        }
    }

    fn expire_locked(state: &mut CallbackState) {
        let now = Instant::now();
        if state
            .active
            .as_ref()
            .is_some_and(|active| now >= active.expires_at_instant)
        {
            state.active = None;
            state.grant = None;
            state.stage = CallbackStage::TimedOut;
            return;
        }
        if state
            .grant
            .as_ref()
            .is_some_and(|grant| now >= grant.expires_at_instant)
        {
            state.grant = None;
            state.stage = CallbackStage::TimedOut;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AcceptError {
    NotActive,
    StateMismatch,
}

fn instance() -> &'static OidcCallbackRuntime {
    static RUNTIME: OnceLock<OidcCallbackRuntime> = OnceLock::new();
    RUNTIME.get_or_init(OidcCallbackRuntime::new)
}

pub(super) fn begin(expected_state: String, expires_at: DateTime<Utc>) -> u64 {
    instance().begin(expected_state, expires_at)
}

pub(super) fn is_active(generation: u64) -> bool {
    instance().is_active(generation)
}

pub(super) fn accept_code(
    generation: u64,
    supplied_state: String,
    authorization_code: String,
) -> Result<(), AcceptError> {
    instance().accept_code(generation, supplied_state, authorization_code)
}

pub(super) fn accept_provider_error(
    generation: u64,
    supplied_state: String,
) -> Result<(), AcceptError> {
    instance().accept_provider_error(generation, supplied_state)
}

pub(super) fn timeout(generation: u64) {
    instance().timeout(generation);
}

pub(super) fn fail(generation: u64) {
    instance().fail(generation);
}

pub(super) fn cancel() -> bool {
    instance().cancel()
}

pub(super) fn status() -> OidcCallbackStatus {
    instance().status()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= left_byte ^ right_byte;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_state_is_accepted_only_once() {
        let runtime = OidcCallbackRuntime::new();
        let generation = runtime.begin(
            "expected-state".to_string(),
            Utc::now() + chrono::Duration::minutes(10),
        );
        assert_eq!(
            runtime.accept_code(
                generation,
                "wrong-state".to_string(),
                "code-one".to_string(),
            ),
            Err(AcceptError::StateMismatch)
        );
        runtime
            .accept_code(
                generation,
                "expected-state".to_string(),
                "code-two".to_string(),
            )
            .expect("matching callback should be accepted");
        assert_eq!(
            runtime.accept_code(
                generation,
                "expected-state".to_string(),
                "code-three".to_string(),
            ),
            Err(AcceptError::NotActive)
        );
        assert!(runtime.status().code_received);
    }
}
