/**
 * Desktop service layer.
 *
 * All native/desktop capability access goes through this module. Components
 * MUST NOT call `@tauri-apps/api` directly — they call the functions exported
 * here. This keeps the UI insulated from the transport.
 *
 * Runtime behavior:
 *   - When running inside Tauri (window.__TAURI_INTERNALS__ present), each
 *     function dispatches to a real Rust command via `invoke(...)`.
 *   - Otherwise (Lovable web preview, `vite dev` in a plain browser) the
 *     mock implementation in `./mock-desktop.ts` is used so the UI is
 *     fully explorable without the native backend.
 *
 * The Rust command names below are the contract the Tauri backend must
 * implement (see `src-tauri/src/commands/`). Do not rename without updating
 * both sides.
 */

import type {
  CameraInfo,
  DisplayInfo,
  MicrophoneInfo,
  RecorderSettings,
  RecordingConfig,
  RecordingOutput,
  WindowInfo,
} from "@/types/recorder";
import * as mock from "./mock-desktop";

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let cachedInvoke: Invoke | null | undefined;

function getInvoke(): Invoke | null {
  if (cachedInvoke !== undefined) return cachedInvoke;
  if (typeof window === "undefined") {
    cachedInvoke = null;
    return null;
  }
  const w = window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke } };
  // Tauri v2 exposes `invoke` directly on the internals object at runtime.
  // We read it dynamically to avoid a static import of `@tauri-apps/api`
  // (which is only installed inside the Tauri build, not in Lovable's web preview).
  const nativeInvoke = w.__TAURI_INTERNALS__?.invoke;
  cachedInvoke = typeof nativeInvoke === "function" ? nativeInvoke : null;
  return cachedInvoke;
}

async function call<T>(cmd: string, args?: Record<string, unknown>, fallback?: () => Promise<T>): Promise<T> {
  const invoke = getInvoke();
  if (invoke) return invoke<T>(cmd, args);
  if (!fallback) {
    throw new Error(`Desktop command '${cmd}' is not available in web preview.`);
  }
  return fallback();
}

export const isDesktop = (): boolean => getInvoke() !== null;

// ---------- device enumeration ----------

export const listDisplays = () =>
  call<DisplayInfo[]>("list_displays", undefined, () => mock.listDisplays());

export const listWindows = () =>
  call<WindowInfo[]>("list_windows", undefined, () => mock.listWindows());

export const listMicrophones = () =>
  call<MicrophoneInfo[]>("list_microphones", undefined, () => mock.listMicrophones());

export const listCameras = () =>
  call<CameraInfo[]>("list_cameras", undefined, () => mock.listCameras());

// ---------- recording lifecycle ----------

export const startRecording = (config: RecordingConfig) =>
  call<{ id: string }>("start_recording", { config }, () => mock.startRecording(config));

export const pauseRecording = () =>
  call<void>("pause_recording", undefined, () => mock.pauseRecording());

export const resumeRecording = () =>
  call<void>("resume_recording", undefined, () => mock.resumeRecording());

export const stopRecording = () =>
  call<RecordingOutput>("stop_recording", undefined, () => mock.stopRecording());

// ---------- library ----------

export const getRecordings = () =>
  call<RecordingOutput[]>("get_recordings", undefined, () => mock.getRecordings());

export const getRecording = (id: string) =>
  call<RecordingOutput | null>("get_recording", { id }, () => mock.getRecording(id));

export const deleteRecording = (id: string) =>
  call<void>("delete_recording", { id }, () => mock.deleteRecording(id));

export const renameRecording = (id: string, title: string) =>
  call<RecordingOutput>("rename_recording", { id, title }, () => mock.renameRecording(id, title));

export const openRecordingLocation = (id: string) =>
  call<void>("open_recording_location", { id }, () => mock.openRecordingLocation(id));

// ---------- settings ----------

export const getSettings = () =>
  call<RecorderSettings>("get_settings", undefined, () => mock.getSettings());

export const updateSettings = (settings: RecorderSettings) =>
  call<RecorderSettings>("update_settings", { settings }, () => mock.updateSettings(settings));

// ---------- live signals (mic level, camera preview) ----------
// In Tauri these will be Rust events; in the web preview they are simulated.

export const subscribeMicLevel = (
  micId: string | null,
  cb: (level: number) => void,
): (() => void) => {
  if (!isDesktop()) return mock.subscribeMicLevel(micId, cb);
  // TODO(native): subscribe to a Tauri event `mic-level` filtered by micId.
  return () => undefined;
};
