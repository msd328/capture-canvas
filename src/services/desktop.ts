import type {
  CameraInfo,
  CapturePreview,
  CaptureTarget,
  DisplayInfo,
  LibrarySnapshot,
  MicrophoneInfo,
  RecorderSettings,
  RecordingConfig,
  RecordingOutput,
  WindowInfo,
} from "@/types/recorder";
import type {
  OidcAuthorizationPreparation,
  OidcCallbackStatus,
  OidcClientStatus,
  OidcSignInLaunch,
  OidcTransactionProbe,
  OidcTransactionStatus,
  SecureAuthProbe,
  SecureAuthStatus,
} from "@/types/saas";
import * as mock from "./mock-desktop";

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type TauriInternals = {
  invoke?: Invoke;
  convertFileSrc?: (filePath: string, protocol?: string) => string;
};

let cachedInvoke: Invoke | null | undefined;

function internals(): TauriInternals | undefined {
  if (typeof window === "undefined") return undefined;
  return (window as unknown as { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__;
}

function getInvoke(): Invoke | null {
  if (cachedInvoke !== undefined) return cachedInvoke;
  const nativeInvoke = internals()?.invoke;
  cachedInvoke = typeof nativeInvoke === "function" ? nativeInvoke : null;
  return cachedInvoke;
}

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
  fallback?: () => Promise<T>,
): Promise<T> {
  const invoke = getInvoke();
  if (invoke) return invoke<T>(cmd, args);
  if (!fallback) throw new Error(`Desktop command '${cmd}' is not available in web preview.`);
  return fallback();
}

export const isDesktop = (): boolean => getInvoke() !== null;

export function localFileUrl(filePath: string): string | null {
  const convert = internals()?.convertFileSrc;
  if (typeof convert !== "function") return null;
  return convert(filePath, "asset");
}

export const getSecureAuthStatus = () =>
  call<SecureAuthStatus>("get_secure_auth_status", undefined, async () => ({
    supported: false,
    signedIn: false,
    storage: "unsupported",
  }));
export const probeSecureAuthStore = () =>
  call<SecureAuthProbe>("probe_secure_auth_store", undefined, async () => ({
    supported: false,
    roundTripOk: false,
    storage: "unsupported",
  }));
export const clearSecureAuthSession = () =>
  call<void>("clear_secure_auth_session", undefined, async () => undefined);
export const getOidcClientStatus = () =>
  call<OidcClientStatus>("get_oidc_client_status", undefined, async () => ({
    configured: false,
    authorizationEndpointHttps: false,
    callbackMode: null,
    scopeCount: 0,
  }));
export const startOidcSignIn = () => call<OidcSignInLaunch>("start_oidc_sign_in");
export const getOidcCallbackStatus = () =>
  call<OidcCallbackStatus>("get_oidc_callback_status", undefined, async () => ({
    stage: "idle",
    pending: false,
    codeReceived: false,
    providerError: false,
    expiresAt: null,
  }));
export const prepareOidcTransaction = () =>
  call<OidcAuthorizationPreparation>("prepare_oidc_transaction");
export const getOidcTransactionStatus = () =>
  call<OidcTransactionStatus>("get_oidc_transaction_status", undefined, async () => ({
    pending: false,
    expiresAt: null,
    expiresInSeconds: 0,
  }));
export const cancelOidcTransaction = () =>
  call<void>("cancel_oidc_transaction", undefined, async () => undefined);
export const probeOidcTransaction = () =>
  call<OidcTransactionProbe>("probe_oidc_transaction", undefined, async () => ({
    s256Ready: false,
    stateRoundTripOk: false,
    nonceRetained: false,
    replayRejected: false,
    verifierKeptNative: false,
  }));

export const listDisplays = () =>
  call<DisplayInfo[]>("list_displays", undefined, () => mock.listDisplays());
export const listWindows = () =>
  call<WindowInfo[]>("list_windows", undefined, () => mock.listWindows());
export const listMicrophones = () =>
  call<MicrophoneInfo[]>("list_microphones", undefined, () => mock.listMicrophones());
export const listCameras = () =>
  call<CameraInfo[]>("list_cameras", undefined, () => mock.listCameras());
export const getSystemAudioSupported = () =>
  call<boolean>("system_audio_supported", undefined, async () => false);
export const captureSourcePreview = (target: CaptureTarget) =>
  call<CapturePreview>("capture_source_preview", { target }, () =>
    mock.captureSourcePreview(target),
  );

export const startRecording = (config: RecordingConfig) =>
  call<{ id: string }>("start_recording", { config }, () => mock.startRecording(config));
export const pauseRecording = () =>
  call<void>("pause_recording", undefined, () => mock.pauseRecording());
export const resumeRecording = () =>
  call<void>("resume_recording", undefined, () => mock.resumeRecording());
export const stopRecording = () =>
  call<RecordingOutput>("stop_recording", undefined, () => mock.stopRecording());

export const getRecordings = () =>
  call<RecordingOutput[]>("get_recordings", undefined, () => mock.getRecordings());
export const getLibrarySnapshot = () =>
  call<LibrarySnapshot>("get_library_snapshot", undefined, async () => ({
    revision: 0,
    recordings: await mock.getRecordings(),
  }));
export const waitForLibraryUpdate = (afterRevision: number) =>
  call<LibrarySnapshot>("wait_for_library_update", { afterRevision }, async () => ({
    revision: afterRevision,
    recordings: await mock.getRecordings(),
  }));
export const getRecording = (id: string) =>
  call<RecordingOutput | null>("get_recording", { id }, () => mock.getRecording(id));
export const deleteRecording = (id: string) =>
  call<void>("delete_recording", { id }, () => mock.deleteRecording(id));
export const renameRecording = (id: string, title: string) =>
  call<RecordingOutput>("rename_recording", { id, title }, () => mock.renameRecording(id, title));
export const openRecordingLocation = (id: string) =>
  call<void>("open_recording_location", { id }, () => mock.openRecordingLocation(id));

export const getSettings = () =>
  call<RecorderSettings>("get_settings", undefined, () => mock.getSettings());
export const updateSettings = (settings: RecorderSettings) =>
  call<RecorderSettings>("update_settings", { settings }, () => mock.updateSettings(settings));
