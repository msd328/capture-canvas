import { useEffect, useRef, useState } from "react";
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

function normalizeDeviceName(value: string): string {
  return value
    .toLowerCase()
    .replace(/^dshow-camera:/, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

export function CameraSelector({
  enabled,
  onEnabledChange,
  cameraId,
  onCameraChange,
  previewActive = true,
}: Props) {
  const [cameras, setCameras] = useState<CameraInfo[]>([]);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    desktop.listCameras().then((list) => {
      if (cancelled) return;
      setCameras(list);
      if (!cameraId && list.length) onCameraChange(list.find((c) => c.isDefault)?.id ?? list[0].id);
    });
    return () => {
      cancelled = true;
    };
    // Device enumeration does not need to run again when selection changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!enabled || !cameraId || !previewActive || !desktop.isDesktop()) {
      setPreviewError(null);
      if (videoRef.current) videoRef.current.srcObject = null;
      return;
    }

    if (!navigator.mediaDevices?.getUserMedia) {
      setPreviewError("Camera preview is unavailable in this WebView.");
      return;
    }

    let cancelled = false;
    let stream: MediaStream | null = null;

    const startPreview = async () => {
      try {
        // Ask once so WebView2 can expose device labels, then choose the browser
        // camera whose label most closely matches the DirectShow camera selected
        // for the native recording pipeline.
        const permissionStream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        const devices = await navigator.mediaDevices.enumerateDevices();
        permissionStream.getTracks().forEach((track) => track.stop());

        const selected = cameras.find((camera) => camera.id === cameraId);
        const wanted = normalizeDeviceName(selected?.name ?? cameraId);
        const browserCamera = devices.find((device) => {
          if (device.kind !== "videoinput") return false;
          const label = normalizeDeviceName(device.label);
          return label === wanted || label.includes(wanted) || wanted.includes(label);
        });

        stream = await navigator.mediaDevices.getUserMedia({
          video: browserCamera?.deviceId
            ? { deviceId: { exact: browserCamera.deviceId }, width: { ideal: 640 }, height: { ideal: 360 } }
            : { width: { ideal: 640 }, height: { ideal: 360 } },
          audio: false,
        });

        if (cancelled) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          await videoRef.current.play().catch(() => undefined);
        }
        setPreviewError(null);
      } catch (error) {
        if (!cancelled) {
          setPreviewError((error as Error).message || "Camera preview unavailable");
        }
      }
    };

    void startPreview();
    return () => {
      cancelled = true;
      if (videoRef.current) videoRef.current.srcObject = null;
      stream?.getTracks().forEach((track) => track.stop());
    };
  }, [enabled, cameraId, previewActive, cameras]);

  return (
    <SectionShell
      icon={enabled ? <Camera className="size-4" /> : <CameraOff className="size-4" />}
      title="Camera"
      trailing={<Switch checked={enabled} onCheckedChange={onEnabledChange} />}
    >
      <div className="flex items-center gap-4">
        <div className="relative flex size-24 shrink-0 items-center justify-center overflow-hidden rounded-full border-2 border-border bg-muted">
          {enabled && previewActive ? (
            <video ref={videoRef} muted playsInline className="size-full object-cover" />
          ) : enabled ? (
            <div className="flex size-full flex-col items-center justify-center bg-[var(--gradient-accent)] text-accent-foreground">
              <Camera className="size-6" />
              <span className="mt-1 text-[10px] font-medium uppercase tracking-wider opacity-80">Recording</span>
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
              Live preview uses one persistent camera stream; recording still uses the native desktop pipeline.
            </p>
          )}
        </div>
      </div>
    </SectionShell>
  );
}
