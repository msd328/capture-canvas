import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { Circle, Sparkles } from "lucide-react";
import { AppShell } from "@/components/AppShell";
import { SourceSelector } from "@/components/recorder/SourceSelector";
import { MicSelector } from "@/components/recorder/MicSelector";
import { CameraSelector } from "@/components/recorder/CameraSelector";
import { SystemAudioToggle } from "@/components/recorder/SystemAudioToggle";
import { Countdown } from "@/components/recorder/Countdown";
import { FloatingControls } from "@/components/recorder/FloatingControls";
import { Button } from "@/components/ui/button";
import { useRecorder } from "@/hooks/useRecorder";
import { getSettings } from "@/services/desktop";
import type { CaptureTarget } from "@/types/recorder";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Recorder — Capture your screen, beautifully" },
      { name: "description", content: "A premium, minimal desktop screen recorder. Capture displays, windows, camera, and audio, then save locally as MP4." },
      { property: "og:title", content: "Recorder — Capture your screen, beautifully" },
      { property: "og:description", content: "A premium, minimal desktop screen recorder." },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
    ],
  }),
  component: RecorderPage,
});

function detectSystemAudioSupport(): { supported: boolean; note: string } {
  return {
    supported: false,
    note: "System audio is being implemented with Windows WASAPI loopback. This build records screen/window video, microphone, and optional camera; the toggle stays disabled so it cannot silently produce a recording without system sound.",
  };
}

function RecorderPage() {
  const navigate = useNavigate();
  const recorder = useRecorder();
  const [target, setTarget] = useState<CaptureTarget | null>(null);
  const [micOn, setMicOn] = useState(true);
  const [micId, setMicId] = useState<string | null>(null);
  const [cameraOn, setCameraOn] = useState(false);
  const [cameraId, setCameraId] = useState<string | null>(null);
  const [systemAudioOn, setSystemAudioOn] = useState(false);
  const [fps, setFps] = useState(30);

  const audioSupport = useMemo(() => detectSystemAudioSupport(), []);

  useEffect(() => {
    getSettings().then((s) => {
      setFps(s.fps);
      if (!micId) setMicId(s.defaultMicrophoneId);
      if (!cameraId) setCameraId(s.defaultCameraId);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const canRecord = !!target && recorder.status === "idle";

  const handleStart = async () => {
    if (!target) return;
    await recorder.start({
      target,
      microphoneId: micOn ? micId : null,
      cameraId: cameraOn ? cameraId : null,
      systemAudio: systemAudioOn && audioSupport.supported,
      fps,
    });
  };

  const handleStop = async () => {
    const out = await recorder.stop();
    if (out) navigate({ to: "/recording/$id", params: { id: out.id } });
  };

  const showFloating = recorder.status === "recording" || recorder.status === "paused" || recorder.status === "stopping";

  return (
    <AppShell>
      <div className="mb-8">
        <div className="inline-flex items-center gap-2 rounded-full border border-border bg-surface px-3 py-1 text-xs text-muted-foreground">
          <Sparkles className="size-3" /> Phase 1 · Local recording
        </div>
        <h1 className="mt-3 text-3xl font-semibold tracking-tight text-foreground">New recording</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Choose a source, set up your audio and camera, then press record.
        </p>
      </div>

      <section className="mb-6 surface-card p-6">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-sm font-semibold text-foreground">Screen</h2>
          {target && (
            <span className="rounded-full bg-muted px-2.5 py-0.5 text-xs text-muted-foreground">
              {target.kind === "display" ? "Display" : "Window"} selected
            </span>
          )}
        </div>
        <SourceSelector value={target} onChange={setTarget} />
      </section>

      <div className="grid gap-4 md:grid-cols-2">
        <MicSelector enabled={micOn} onEnabledChange={setMicOn} micId={micId} onMicChange={setMicId} />
        <CameraSelector enabled={cameraOn} onEnabledChange={setCameraOn} cameraId={cameraId} onCameraChange={setCameraId} />
        <div className="md:col-span-2">
          <SystemAudioToggle
            enabled={systemAudioOn}
            onEnabledChange={setSystemAudioOn}
            supported={audioSupport.supported}
            platformNote={audioSupport.note}
          />
        </div>
      </div>

      <div className="mt-10 flex flex-col items-center gap-3">
        <Button
          size="lg"
          disabled={!canRecord}
          onClick={handleStart}
          className="group h-14 gap-3 rounded-full bg-record px-8 text-record-foreground shadow-[var(--shadow-record)] hover:bg-record/90 disabled:opacity-40 disabled:shadow-none"
        >
          <span className="flex size-4 items-center justify-center rounded-full border-2 border-current">
            <Circle className="size-2 fill-current" />
          </span>
          <span className="text-base font-semibold">Start Recording</span>
        </Button>
        {!target && (
          <p className="text-xs text-muted-foreground">Select a screen or window to enable recording.</p>
        )}
        {recorder.error && (
          <p className="text-xs text-destructive">Error: {recorder.error.message}</p>
        )}
      </div>

      {recorder.status === "countdown" && recorder.countdown !== null && <Countdown value={recorder.countdown} />}

      {showFloating && (
        <FloatingControls
          status={recorder.status}
          elapsedMs={recorder.elapsedMs}
          micOn={micOn}
          cameraOn={cameraOn}
          onPause={recorder.pause}
          onResume={recorder.resume}
          onStop={handleStop}
        />
      )}
    </AppShell>
  );
}
