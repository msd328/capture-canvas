import { useCallback, useEffect, useRef, useState } from "react";
import * as desktop from "@/services/desktop";
import type {
  RecorderError,
  RecordingConfig,
  RecordingOutput,
  RecordingStatus,
} from "@/types/recorder";

interface UseRecorderResult {
  status: RecordingStatus;
  elapsedMs: number;
  error: RecorderError | null;
  start: (config: RecordingConfig) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  stop: () => Promise<RecordingOutput | null>;
  reset: () => void;
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  try {
    const text = String(error);
    if (text && text !== "[object Object]") return text;
  } catch {
    // Fall through to the user-friendly default.
  }
  return fallback;
}

export function useRecorder(): UseRecorderResult {
  const [status, setStatus] = useState<RecordingStatus>("idle");
  const [error, setError] = useState<RecorderError | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const startedAtRef = useRef<number | null>(null);
  const pausedAccumRef = useRef(0);
  const pausedAtRef = useRef<number | null>(null);
  const transitionRef = useRef(false);

  const updateElapsed = useCallback(() => {
    if (startedAtRef.current !== null && pausedAtRef.current === null) {
      setElapsedMs(Date.now() - startedAtRef.current - pausedAccumRef.current);
    }
  }, []);

  useEffect(() => {
    if (status !== "recording") return;

    // The previous requestAnimationFrame loop re-rendered the entire recording
    // page about 60 times per second. A recorder clock only needs a few updates
    // per second, and lowering this frequency keeps control clicks responsive.
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 250);
    return () => window.clearInterval(timer);
  }, [status, updateElapsed]);

  const start = useCallback(async (config: RecordingConfig) => {
    if (transitionRef.current) return;
    transitionRef.current = true;
    setError(null);
    setStatus("preparing");

    try {
      // Start immediately. A configurable countdown can be reintroduced later,
      // but it should not add unavoidable latency to every recording.
      await desktop.startRecording(config);
      startedAtRef.current = Date.now();
      pausedAccumRef.current = 0;
      pausedAtRef.current = null;
      setElapsedMs(0);
      setStatus("recording");
    } catch (e) {
      const message = errorMessage(e, "Failed to start recording");
      console.error("Recorder start failed:", e);
      setError({ code: "InitFailed", message });
      setStatus("idle");
    } finally {
      transitionRef.current = false;
    }
  }, []);

  const pause = useCallback(async () => {
    if (transitionRef.current) return;
    transitionRef.current = true;
    setError(null);

    const now = Date.now();
    if (startedAtRef.current !== null) {
      setElapsedMs(now - startedAtRef.current - pausedAccumRef.current);
    }
    pausedAtRef.current = now;
    setStatus("pausing");

    try {
      await desktop.pauseRecording();
      setStatus("paused");
    } catch (e) {
      const message = errorMessage(e, "Failed to pause recording");
      console.error("Recorder pause failed:", e);
      pausedAtRef.current = null;
      setError({ code: "EncoderFailed", message });
      setStatus("recording");
    } finally {
      transitionRef.current = false;
    }
  }, []);

  const resume = useCallback(async () => {
    if (transitionRef.current) return;
    transitionRef.current = true;
    setError(null);
    setStatus("resuming");

    try {
      await desktop.resumeRecording();
      if (pausedAtRef.current !== null) {
        pausedAccumRef.current += Date.now() - pausedAtRef.current;
        pausedAtRef.current = null;
      }
      setStatus("recording");
    } catch (e) {
      const message = errorMessage(e, "Failed to resume recording");
      console.error("Recorder resume failed:", e);
      setError({ code: "EncoderFailed", message });
      setStatus("paused");
    } finally {
      transitionRef.current = false;
    }
  }, []);

  const stop = useCallback(async () => {
    if (transitionRef.current) return null;
    transitionRef.current = true;
    setError(null);
    setStatus("stopping");

    try {
      const out = await desktop.stopRecording();
      setStatus("idle");
      startedAtRef.current = null;
      pausedAccumRef.current = 0;
      pausedAtRef.current = null;
      setElapsedMs(0);
      return out;
    } catch (e) {
      const message = errorMessage(e, "Failed to finalize recording");
      console.error("Recorder stop failed:", e);
      setError({ code: "EncoderFailed", message });
      setStatus("error");
      return null;
    } finally {
      transitionRef.current = false;
    }
  }, []);

  const reset = useCallback(() => {
    transitionRef.current = false;
    setStatus("idle");
    setError(null);
    setElapsedMs(0);
    startedAtRef.current = null;
    pausedAccumRef.current = 0;
    pausedAtRef.current = null;
  }, []);

  return { status, elapsedMs, error, start, pause, resume, stop, reset };
}

export function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}
