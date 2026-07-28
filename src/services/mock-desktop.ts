/**
 * Web-preview mock implementation of the desktop service surface.
 *
 * This lets designers/developers exercise the full UI in a browser without
 * the Tauri/Rust backend attached. It is NOT a recording implementation —
 * `startRecording` returns a synthetic id and `stopRecording` returns a
 * synthetic RecordingOutput; no bytes are captured.
 *
 * Do not import this module from components. Always go through
 * `@/services/desktop`.
 */

import type {
  CameraInfo,
  CapturePreview,
  CaptureTarget,
  DisplayInfo,
  MicrophoneInfo,
  RecorderSettings,
  RecordingConfig,
  RecordingOutput,
  WindowInfo,
} from "@/types/recorder";

const LS_RECORDINGS = "recorder.mock.recordings";
const LS_SETTINGS = "recorder.mock.settings";

function readLS<T>(key: string, fallback: T): T {
  if (typeof localStorage === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function writeLS<T>(key: string, value: T) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(key, JSON.stringify(value));
}

// ---------- device stubs ----------

export async function listDisplays(): Promise<DisplayInfo[]> {
  return [
    { id: "display-1", name: "Built-in Display", width: 2560, height: 1600, isPrimary: true },
    { id: "display-2", name: "External Monitor", width: 3840, height: 2160, isPrimary: false },
  ];
}

export async function listWindows(): Promise<WindowInfo[]> {
  return [
    { id: "win-1", name: "Design Review — Figma", appName: "Figma", width: 1440, height: 900 },
    { id: "win-2", name: "index.tsx — VS Code", appName: "VS Code", width: 1600, height: 1000 },
    { id: "win-3", name: "Team Sync — Zoom", appName: "Zoom", width: 1280, height: 720 },
    { id: "win-4", name: "Recorder Docs — Chrome", appName: "Chrome", width: 1440, height: 900 },
  ];
}

export async function listMicrophones(): Promise<MicrophoneInfo[]> {
  return [
    { id: "mic-default", name: "System Default", isDefault: true },
    { id: "mic-builtin", name: "MacBook Pro Microphone", isDefault: false },
    { id: "mic-usb", name: "Shure MV7", isDefault: false },
  ];
}

export async function listCameras(): Promise<CameraInfo[]> {
  return [
    { id: "cam-facetime", name: "FaceTime HD Camera", isDefault: true },
    { id: "cam-external", name: "Logitech Brio", isDefault: false },
  ];
}

export async function captureSourcePreview(target: CaptureTarget): Promise<CapturePreview> {
  const source =
    target.kind === "display"
      ? (await listDisplays()).find((item) => item.id === target.id)
      : (await listWindows()).find((item) => item.id === target.id);
  const width = source?.width ?? 1920;
  const height = source?.height ?? 1080;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="100%" height="100%" fill="#202536"/><text x="50%" y="50%" text-anchor="middle" fill="white" font-family="sans-serif" font-size="48">Source preview</text></svg>`;
  return {
    dataUrl: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`,
    width,
    height,
  };
}

// ---------- recording state ----------

let activeRecording: { id: string; startedAt: number; pausedMs: number; pausedAt: number | null; config: RecordingConfig } | null = null;

export async function startRecording(config: RecordingConfig): Promise<{ id: string }> {
  const id = crypto.randomUUID();
  activeRecording = { id, startedAt: Date.now(), pausedMs: 0, pausedAt: null, config };
  return { id };
}
export async function pauseRecording(): Promise<void> {
  if (activeRecording && !activeRecording.pausedAt) activeRecording.pausedAt = Date.now();
}
export async function resumeRecording(): Promise<void> {
  if (activeRecording?.pausedAt) {
    activeRecording.pausedMs += Date.now() - activeRecording.pausedAt;
    activeRecording.pausedAt = null;
  }
}
export async function stopRecording(): Promise<RecordingOutput> {
  if (!activeRecording) throw new Error("No active recording");
  const now = Date.now();
  const paused = activeRecording.pausedMs + (activeRecording.pausedAt ? now - activeRecording.pausedAt : 0);
  const duration = now - activeRecording.startedAt - paused;
  const output: RecordingOutput = {
    id: activeRecording.id,
    title: activeRecording.config.title ?? defaultTitle(),
    filePath: `~/Recordings/${activeRecording.id}.mp4`,
    createdAt: new Date().toISOString(),
    durationMs: Math.max(1000, duration),
    width: activeRecording.config.cropRegion?.width ?? 1920,
    height: activeRecording.config.cropRegion?.height ?? 1080,
    fileSizeBytes: Math.round((duration / 1000) * 1.8 * 1024 * 1024),
  };
  const all = readLS<RecordingOutput[]>(LS_RECORDINGS, []);
  all.unshift(output);
  writeLS(LS_RECORDINGS, all);
  activeRecording = null;
  return output;
}

function defaultTitle() {
  const d = new Date();
  return `Recording ${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

// ---------- library ----------

export async function getRecordings(): Promise<RecordingOutput[]> {
  return readLS<RecordingOutput[]>(LS_RECORDINGS, []);
}
export async function getRecording(id: string): Promise<RecordingOutput | null> {
  return readLS<RecordingOutput[]>(LS_RECORDINGS, []).find((r) => r.id === id) ?? null;
}
export async function deleteRecording(id: string): Promise<void> {
  writeLS(LS_RECORDINGS, readLS<RecordingOutput[]>(LS_RECORDINGS, []).filter((r) => r.id !== id));
}
export async function renameRecording(id: string, title: string): Promise<RecordingOutput> {
  const all = readLS<RecordingOutput[]>(LS_RECORDINGS, []);
  const updated = all.map((r) => (r.id === id ? { ...r, title } : r));
  writeLS(LS_RECORDINGS, updated);
  const found = updated.find((r) => r.id === id);
  if (!found) throw new Error("Recording not found");
  return found;
}
export async function openRecordingLocation(_id: string): Promise<void> {
  // TODO(native): reveal file in Finder/Explorer via Tauri.
}

// ---------- settings ----------

const DEFAULT_SETTINGS: RecorderSettings = {
  defaultMicrophoneId: null,
  defaultCameraId: null,
  fps: 30,
  outputDirectory: "~/Recordings",
  launchAtStartup: false,
  showCameraBubble: true,
};

export async function getSettings(): Promise<RecorderSettings> {
  return readLS<RecorderSettings>(LS_SETTINGS, DEFAULT_SETTINGS);
}
export async function updateSettings(settings: RecorderSettings): Promise<RecorderSettings> {
  writeLS(LS_SETTINGS, settings);
  return settings;
}

// ---------- simulated mic level ----------

export function subscribeMicLevel(micId: string | null, cb: (level: number) => void): () => void {
  if (!micId) return () => undefined;
  let raf = 0;
  let t = 0;
  const tick = () => {
    t += 0.08;
    const base = 0.35 + Math.sin(t) * 0.15 + Math.sin(t * 3.1) * 0.1;
    const jitter = Math.random() * 0.15;
    cb(Math.max(0, Math.min(1, base + jitter)));
    raf = requestAnimationFrame(tick);
  };
  raf = requestAnimationFrame(tick);
  return () => cancelAnimationFrame(raf);
}
