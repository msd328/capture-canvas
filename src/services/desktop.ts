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

async function call<T>(cmd: string, args?: Record<string, unknown>, fallback?: () => Promise<T>): Promise<T> {
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

export const listDisplays = () => call<DisplayInfo[]>("list_displays", undefined, () => mock.listDisplays());
export const listWindows = () => call<WindowInfo[]>("list_windows", undefined, () => mock.listWindows());
export const listMicrophones = () => call<MicrophoneInfo[]>("list_microphones", undefined, () => mock.listMicrophones());
export const listCameras = () => call<CameraInfo[]>("list_cameras", undefined, () => mock.listCameras());
export const getCameraPreviewFrame = (cameraId: string) =>
  call<string>("get_camera_preview_frame", { cameraId });
export const getSystemAudioSupported = () =>
  call<boolean>("system_audio_supported", undefined, async () => true);

export const startRecording = (config: RecordingConfig) =>
  call<{ id: string }>("start_recording", { config }, () => mock.startRecording(config));
export const pauseRecording = () => call<void>("pause_recording", undefined, () => mock.pauseRecording());
export const resumeRecording = () => call<void>("resume_recording", undefined, () => mock.resumeRecording());
export const stopRecording = () => call<RecordingOutput>("stop_recording", undefined, () => mock.stopRecording());

export const getRecordings = () => call<RecordingOutput[]>("get_recordings", undefined, () => mock.getRecordings());
export const getRecording = (id: string) => call<RecordingOutput | null>("get_recording", { id }, () => mock.getRecording(id));
export const deleteRecording = (id: string) => call<void>("delete_recording", { id }, () => mock.deleteRecording(id));
export const renameRecording = (id: string, title: string) =>
  call<RecordingOutput>("rename_recording", { id, title }, () => mock.renameRecording(id, title));
export const openRecordingLocation = (id: string) =>
  call<void>("open_recording_location", { id }, () => mock.openRecordingLocation(id));

export const getSettings = () => call<RecorderSettings>("get_settings", undefined, () => mock.getSettings());
export const updateSettings = (settings: RecorderSettings) =>
  call<RecorderSettings>("update_settings", { settings }, () => mock.updateSettings(settings));

export const subscribeMicLevel = (
  micId: string | null,
  cb: (level: number) => void,
): (() => void) => {
  if (!isDesktop()) return mock.subscribeMicLevel(micId, cb);
  cb(0);
  return () => undefined;
};
