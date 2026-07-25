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
}

export function CameraSelector({ enabled, onEnabledChange, cameraId, onCameraChange }: Props) {
  const [cameras, setCameras] = useState<CameraInfo[]>([]);

  useEffect(() => {
    desktop.listCameras().then((list) => {
      setCameras(list);
      if (!cameraId && list.length) onCameraChange(list.find((c) => c.isDefault)?.id ?? list[0].id);
    });
  }, [cameraId, onCameraChange]);

  return (
    <SectionShell
      icon={enabled ? <Camera className="size-4" /> : <CameraOff className="size-4" />}
      title="Camera"
      trailing={<Switch checked={enabled} onCheckedChange={onEnabledChange} />}
    >
      <div className="flex items-center gap-4">
        <div className="relative flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-full border-2 border-border bg-muted">
          {enabled ? (
            <div className="flex size-full flex-col items-center justify-center bg-[var(--gradient-accent)] text-accent-foreground">
              <Camera className="size-6" />
              <span className="mt-1 text-[10px] font-medium uppercase tracking-wider opacity-80">Preview</span>
            </div>
          ) : (
            <CameraOff className="size-6 text-muted-foreground" />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <Select value={cameraId ?? ""} onValueChange={(v) => onCameraChange(v)} disabled={!enabled}>
            <SelectTrigger>
              <SelectValue placeholder="Select camera" />
            </SelectTrigger>
            <SelectContent>
              {cameras.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}
                  {c.isDefault ? " · Default" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="mt-2 text-xs text-muted-foreground">
            Appears as a floating bubble during recording.
          </p>
        </div>
      </div>
    </SectionShell>
  );
}
