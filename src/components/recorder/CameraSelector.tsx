import { useEffect, useState } from "react";
import { Camera, CameraOff } from "lucide-react";
import * as desktop from "@/services/desktop";
import type { CameraInfo } from "@/types/recorder";
import { Switch } from "@/components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { SectionShell } from "./MicSelector";

interface Props {
  enabled: boolean;
  onEnabledChange: (v: boolean) => void;
  cameraId: string | null;
  onCameraChange: (id: string | null) => void;
  previewActive?: boolean;
}

export function CameraSelector({
  enabled,
  onEnabledChange,
  cameraId,
  onCameraChange,
  previewActive = true,
}: Props) {
  const [cameras, setCameras] = useState<CameraInfo[]>([]);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);

  useEffect(() => {
    desktop.listCameras().then((list) => {
      setCameras(list);
      if (!cameraId && list.length) onCameraChange(list.find((c) => c.isDefault)?.id ?? list[0].id);
    });
  }, [cameraId, onCameraChange]);

  useEffect(() => {
    if (!enabled || !cameraId || !previewActive || !desktop.isDesktop()) {
      setPreview(null);
      setPreviewError(null);
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const refresh = async () => {
      try {
        const frame = await desktop.getCameraPreviewFrame(cameraId);
        if (!cancelled) {
          setPreview(frame);
          setPreviewError(null);
        }
      } catch (error) {
        if (!cancelled) setPreviewError((error as Error).message || "Camera preview unavailable");
      }
      if (!cancelled) timer = setTimeout(refresh, 1000);
    };

    refresh();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [enabled, cameraId, previewActive]);

  return (
    <SectionShell
      icon={enabled ? <Camera className="size-4" /> : <CameraOff className="size-4" />}
      title="Camera"
      trailing={<Switch checked={enabled} onCheckedChange={onEnabledChange} />}
    >
      <div className="flex items-center gap-4">
        <div className="relative flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-full border-2 border-border bg-muted">
          {enabled && preview ? (
            <img src={preview} alt="Camera preview" className="size-full object-cover" />
          ) : enabled ? (
            <div className="flex size-full flex-col items-center justify-center bg-[var(--gradient-accent)] text-accent-foreground">
              <Camera className="size-6" />
              <span className="mt-1 text-[10px] font-medium uppercase tracking-wider opacity-80">
                {previewActive ? "Loading" : "Recording"}
              </span>
            </div>
          ) : (
            <CameraOff className="size-6 text-muted-foreground" />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <Select value={cameraId ?? ""} onValueChange={(v) => onCameraChange(v)} disabled={!enabled}>
            <SelectTrigger>
              <SelectValue placeholder={cameras.length ? "Select camera" : "No camera found"} />
            </SelectTrigger>
            <SelectContent>
              {cameras.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}{c.isDefault ? " · Default" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {previewError ? (
            <p className="mt-2 text-xs text-destructive">{previewError}</p>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">
              Camera is previewed here and appears as an overlay during recording.
            </p>
          )}
        </div>
      </div>
    </SectionShell>
  );
}
