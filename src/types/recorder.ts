// Shared types between the React frontend and the Tauri/Rust backend.
// The Rust side mirrors these via serde in `src-tauri/src/recording/types.rs`.

export type CaptureKind = "display" | "window";

export interface DisplayInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  isPrimary: boolean;
  thumbnailDataUrl?: string;
}

export interface WindowInfo {
  id: string;
  name: string;
  appName: string;
  width: number;
  height: number;
  thumbnailDataUrl?: string;
}

export interface MicrophoneInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface CameraInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface CaptureTarget {
  kind: CaptureKind;
  id: string;
}

export interface CropRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CapturePreview {
  dataUrl: string;
  width: number;
  height: number;
}

export interface RecordingConfig {
  target: CaptureTarget;
  microphoneId: string | null;
  cameraId: string | null;
  systemAudio: boolean;
  fps: number;
  cropRegion?: CropRegion | null;
  outputPath?: string;
  title?: string;
}

export type RecordingStatus =
  | "idle"
  | "preparing"
  | "recording"
  | "pausing"
  | "paused"
  | "resuming"
  | "stopping"
  | "error";

export interface RecordingOutput {
  id: string;
  title: string;
  filePath: string;
  createdAt: string; // ISO
  durationMs: number;
  width: number;
  height: number;
  fileSizeBytes: number;
  thumbnailDataUrl?: string;
}

export interface LibrarySnapshot {
  revision: number;
  recordings: RecordingOutput[];
}

export interface RecorderSettings {
  defaultMicrophoneId: string | null;
  defaultCameraId: string | null;
  fps: number;
  outputDirectory: string;
  launchAtStartup: boolean;
  showCameraBubble: boolean;
}

export type RecorderErrorCode =
  | "PermissionDeniedScreen"
  | "PermissionDeniedMicrophone"
  | "PermissionDeniedCamera"
  | "CameraUnavailable"
  | "MicrophoneUnavailable"
  | "DeviceDisconnected"
  | "InsufficientDiskSpace"
  | "InitFailed"
  | "EncoderFailed"
  | "OutputCreateFailed"
  | "SystemAudioUnsupported"
  | "Unknown";

export interface RecorderError {
  code: RecorderErrorCode;
  message: string;
}
