//! Native paid-access enforcement for protected desktop operations.
//!
//! A release build fails closed unless a fresh backend-confirmed
//! `desktop_full_access` result is cached for the current verified native session.
//! Debug builds may keep enforcement disabled for local recorder development.

use crate::saas_access::{AccountStatus, NativeAccountAccessStatus};
use chrono::Utc;

const BUILD_PAID_ACCESS_REQUIRED: Option<&str> = option_env!("RECORDER_PAID_ACCESS_REQUIRED");

pub(crate) fn require_recording_start_access() -> Result<(), String> {
    let required = enforcement_required()?;
    if !required {
        eprintln!(
            "[Recorder][AccessGateHealth] operation=start_recording ok=true enforcement_required=false debug_bypass=true"
        );
        return Ok(());
    }

    let status = crate::saas_access::status().map_err(|_| {
        deny("access_status_unavailable");
        "Recorder could not verify paid access".to_string()
    })?;
    validate_recording_access(&status).map_err(|code| {
        deny(code);
        "An active Recorder subscription is required to start recording".to_string()
    })?;

    eprintln!(
        "[Recorder][AccessGateHealth] operation=start_recording ok=true enforcement_required=true access_checked=true authenticated=true full_access=true"
    );
    Ok(())
}

fn enforcement_required() -> Result<bool, String> {
    enforcement_required_from_value(BUILD_PAID_ACCESS_REQUIRED, cfg!(debug_assertions)).map_err(
        |code| {
            eprintln!(
                "[Recorder][AccessGateHealth] operation=configuration ok=false code={code}"
            );
            format!("Recorder paid-access enforcement configuration is invalid ({code})")
        },
    )
}

fn enforcement_required_from_value(
    value: Option<&str>,
    debug_build: bool,
) -> Result<bool, &'static str> {
    match value.map(str::trim) {
        None if debug_build => Ok(false),
        None => Ok(true),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") if debug_build => Ok(false),
        Some("0" | "false") => Err("release_enforcement_cannot_be_disabled"),
        Some("") => Err("enforcement_value_empty"),
        Some(_) => Err("enforcement_value_invalid"),
    }
}

fn validate_recording_access(status: &NativeAccountAccessStatus) -> Result<(), &'static str> {
    if !status.configured {
        return Err("access_endpoint_not_configured");
    }
    if !status.checked {
        return Err("access_not_checked");
    }
    if !status.authenticated {
        return Err("session_not_authenticated");
    }
    if status.account_status != Some(AccountStatus::Active) {
        return Err("account_not_active");
    }
    if !status.full_access {
        return Err("desktop_full_access_missing");
    }
    if !status
        .valid_until
        .as_ref()
        .is_some_and(|deadline| *deadline > Utc::now())
    {
        return Err("access_check_expired");
    }
    Ok(())
}

fn deny(code: &str) {
    eprintln!(
        "[Recorder][AccessGateHealth] operation=start_recording ok=false enforcement_required=true code={code}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> NativeAccountAccessStatus {
        NativeAccountAccessStatus {
            configured: true,
            endpoint_https: true,
            loopback_development: false,
            checked: true,
            authenticated: true,
            account_status: Some(AccountStatus::Active),
            subscription: None,
            entitlement_count: 1,
            full_access: true,
            checked_at: Some(Utc::now()),
            valid_until: Some(Utc::now() + chrono::Duration::minutes(5)),
            access_token_native_only: true,
        }
    }

    #[test]
    fn release_builds_fail_closed_by_default() {
        assert_eq!(enforcement_required_from_value(None, false), Ok(true));
        assert_eq!(
            enforcement_required_from_value(Some("false"), false),
            Err("release_enforcement_cannot_be_disabled")
        );
        assert_eq!(
            enforcement_required_from_value(Some("0"), false),
            Err("release_enforcement_cannot_be_disabled")
        );
    }

    #[test]
    fn debug_builds_may_opt_in_or_remain_unconfigured() {
        assert_eq!(enforcement_required_from_value(None, true), Ok(false));
        assert_eq!(
            enforcement_required_from_value(Some("true"), true),
            Ok(true)
        );
        assert_eq!(
            enforcement_required_from_value(Some("false"), true),
            Ok(false)
        );
        assert!(enforcement_required_from_value(Some("yes"), true).is_err());
    }

    #[test]
    fn fresh_active_full_access_is_accepted() {
        assert_eq!(validate_recording_access(&status()), Ok(()));
    }

    #[test]
    fn missing_stale_or_disabled_access_fails_closed() {
        let mut value = status();
        value.full_access = false;
        assert_eq!(
            validate_recording_access(&value),
            Err("desktop_full_access_missing")
        );

        let mut value = status();
        value.valid_until = Some(Utc::now() - chrono::Duration::seconds(1));
        assert_eq!(
            validate_recording_access(&value),
            Err("access_check_expired")
        );

        let mut value = status();
        value.account_status = Some(AccountStatus::Disabled);
        assert_eq!(
            validate_recording_access(&value),
            Err("account_not_active")
        );
    }

    #[test]
    fn serialized_status_never_contains_entitlement_names_or_identity() {
        let serialized = serde_json::to_string(&status()).unwrap();
        assert!(!serialized.contains("desktop_full_access"));
        assert!(!serialized.contains("userId"));
        assert!(!serialized.contains("subject"));
    }
}
