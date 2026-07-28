import { Pause, Play, Square, Mic, MicOff, Camera, CameraOff, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatElapsed } from "@/hooks/useRecorder";
import type { RecordingStatus } from "@/types/recorder";
import { cn } from "@/lib/utils";

interface Props {
  status: RecordingStatus;
  elapsedMs: number;
  micOn: boolean;
  cameraOn: boolean;
  onPause: () => void | Promise<void>;
  onResume: () => void | Promise<void>;
  onStop: () => void | Promise<void>;
}

export function FloatingControls({ status, elapsedMs, micOn, cameraOn, onPause, onResume, onStop }: Props) {
  const paused = status === "paused" || status === "pausing";
  const transitioning = status === "pausing" || status === "resuming" || status === "stopping";

  const stateLabel =
    status === "pausing"
      ? "PAUSING"
      : status === "resuming"
        ? "RESUMING"
        : status === "stopping"
          ? "SAVING"
          : status === "paused"
            ? "PAUSED"
            : "REC";

  return (
    <div className="fixed bottom-6 left-1/2 z-40 -translate-x-1/2">
      <div
        className="flex items-center gap-3 rounded-full border border-border-strong bg-surface/95 px-3 py-2 shadow-[var(--shadow-elevated)] backdrop-blur"
        aria-busy={transitioning}
      >
        <div className="flex items-center gap-2 pl-2 pr-1">
          <span className={cn("rec-dot", paused && "!animate-none opacity-50")} aria-hidden />
          <span className="text-xs font-semibold uppercase tracking-wider text-record">{stateLabel}</span>
          <span className="ml-2 font-mono text-sm tabular-nums text-foreground">{formatElapsed(elapsedMs)}</span>
        </div>

        <div className="h-6 w-px bg-border" />

        {status === "pausing" ? (
          <Button size="sm" variant="ghost" disabled className="gap-1.5">
            <Loader2 className="size-4 animate-spin" /> Pausing…
          </Button>
        ) : status === "resuming" ? (
          <Button size="sm" variant="ghost" disabled className="gap-1.5">
            <Loader2 className="size-4 animate-spin" /> Resuming…
          </Button>
        ) : paused ? (
          <Button size="sm" variant="ghost" onClick={onResume} className="gap-1.5">
            <Play className="size-4" /> Resume
          </Button>
        ) : (
          <Button size="sm" variant="ghost" onClick={onPause} className="gap-1.5">
            <Pause className="size-4" /> Pause
          </Button>
        )}

        <Button
          size="sm"
          variant="destructive"
          onClick={onStop}
          disabled={transitioning}
          className="gap-1.5 rounded-full"
        >
          {status === "stopping" ? (
            <>
              <Loader2 className="size-3.5 animate-spin" /> Saving…
            </>
          ) : (
            <>
              <Square className="size-3.5 fill-current" /> Stop
            </>
          )}
        </Button>

        <div className="h-6 w-px bg-border" />

        <div className="flex items-center gap-2 pl-1 pr-2 text-muted-foreground">
          {micOn ? <Mic className="size-4" /> : <MicOff className="size-4 opacity-50" />}
          {cameraOn ? <Camera className="size-4" /> : <CameraOff className="size-4 opacity-50" />}
        </div>
      </div>
      <p className="mt-2 text-center text-[10px] uppercase tracking-widest text-muted-foreground/70">
        Excluded from capture on supported OS
      </p>
    </div>
  );
}
