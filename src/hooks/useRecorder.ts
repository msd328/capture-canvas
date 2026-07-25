import { useCallback, useEffect, useRef, useState } from "react";
import * as desktop from "@/services/desktop";
import type { RecorderError, RecordingConfig, RecordingOutput, RecordingStatus } from "@/types/recorder";

interface UseRecorderResult {
  status: RecordingStatus;
  elapsedMs: number;
  countdown: number | null;
  error: RecorderError | null;
  start: (config: RecordingConfig) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  stop: () => Promise<RecordingOutput | null>;
  reset: () => void;
}

export function useRecorder(): UseRecorderResult {
  const [status, setStatus] = useState<RecordingStatus>("idle");
  const [countdown, setCountdown] = useState<number | null>(null);
  const [error, setError] = useState<RecorderError | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const startedAtRef = useRef<number | null>(null);
  const pausedAccumRef = useRef(0);
  const pausedAtRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  const tick = useCallback(() => {
    if (startedAtRef.current !== null && pausedAtRef.current === null) {
      setElapsedMs(Date.now() - startedAtRef.current - pausedAccumRef.current);
    }
    rafRef.current = requestAnimationFrame(tick);
  }, []);

  useEffect(() => {
    if (status === "recording" || status === "paused") {
      rafRef.current = requestAnimationFrame(tick);
      return () => {
        if (rafRef.current) cancelAnimationFrame(rafRef.current);
      };
    }
  }, [status, tick]);

  const start = useCallback(async (config: RecordingConfig) => {
    setError(null);
    setStatus("preparing");
    try {
      setStatus("countdown");
      for (let i = 3; i >= 1; i--) {
        setCountdown(i);
        await new Promise((r) => setTimeout(r, 900));
      }
      setCountdown(null);
      await desktop.startRecording(config);
      startedAtRef.current = Date.now();
      pausedAccumRef.current = 0;
      pausedAtRef.current = null;
      setElapsedMs(0);
      setStatus("recording");
    } catch (e) {
      setError({ code: "InitFailed", message: (e as Error).message ?? "Failed to start recording" });
      setStatus("error");
      setCountdown(null);
    }
  }, []);

  const pause = useCallback(async () => {
    await desktop.pauseRecording();
    pausedAtRef.current = Date.now();
    setStatus("paused");
  }, []);

  const resume = useCallback(async () => {
    await desktop.resumeRecording();
    if (pausedAtRef.current) {
      pausedAccumRef.current += Date.now() - pausedAtRef.current;
      pausedAtRef.current = null;
    }
    setStatus("recording");
  }, []);

  const stop = useCallback(async () => {
    setStatus("stopping");
    try {
      const out = await desktop.stopRecording();
      setStatus("idle");
      startedAtRef.current = null;
      setElapsedMs(0);
      return out;
    } catch (e) {
      setError({ code: "EncoderFailed", message: (e as Error).message ?? "Failed to finalize recording" });
      setStatus("error");
      return null;
    }
  }, []);

  const reset = useCallback(() => {
    setStatus("idle");
    setError(null);
    setElapsedMs(0);
    setCountdown(null);
    startedAtRef.current = null;
    pausedAccumRef.current = 0;
    pausedAtRef.current = null;
  }, []);

  return { status, elapsedMs, countdown, error, start, pause, resume, stop, reset };
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
