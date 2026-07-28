import { AppWindow, Crop, Loader2, Monitor, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import * as desktop from "@/services/desktop";
import type {
  CaptureKind,
  CapturePreview,
  CaptureTarget,
  CropRegion,
  DisplayInfo,
  WindowInfo,
} from "@/types/recorder";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

interface Props {
  value: CaptureTarget | null;
  onChange: (target: CaptureTarget) => void;
  cropRegion: CropRegion | null;
  onCropChange: (crop: CropRegion | null) => void;
}

type Point = { x: number; y: number };

export function SourceSelector({ value, onChange, cropRegion, onCropChange }: Props) {
  const [kind, setKind] = useState<CaptureKind>("display");
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [windows, setWindows] = useState<WindowInfo[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [preview, setPreview] = useState<CapturePreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [draft, setDraft] = useState<CropRegion | null>(null);
  const [dragStart, setDragStart] = useState<Point | null>(null);

  const refreshSources = useCallback(async () => {
    setRefreshing(true);
    try {
      const [nextDisplays, nextWindows] = await Promise.all([
        desktop.listDisplays(),
        desktop.listWindows(),
      ]);
      setDisplays(nextDisplays);
      setWindows(nextWindows);
    } finally {
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void refreshSources();
  }, [refreshSources]);

  const selectedSource = useMemo(() => {
    if (!value) return null;
    return value.kind === "display"
      ? displays.find((source) => source.id === value.id) ?? null
      : windows.find((source) => source.id === value.id) ?? null;
  }, [displays, value, windows]);

  const chooseTarget = (target: CaptureTarget) => {
    onChange(target);
    onCropChange(null);
    setPreview(null);
    setDraft(null);
    setPreviewError(null);
  };

  const openAreaSelector = async () => {
    if (!value) return;
    setPreviewLoading(true);
    setPreviewError(null);
    try {
      const nextPreview = await desktop.captureSourcePreview(value);
      setPreview(nextPreview);
      setDraft(
        cropRegion ?? {
          x: 0,
          y: 0,
          width: nextPreview.width,
          height: nextPreview.height,
        },
      );
    } catch (error) {
      setPreviewError(error instanceof Error ? error.message : String(error));
    } finally {
      setPreviewLoading(false);
    }
  };

  const closeAreaSelector = () => {
    setPreview(null);
    setDraft(null);
    setDragStart(null);
    setPreviewError(null);
  };

  const sourcePoint = (event: React.PointerEvent<HTMLDivElement>): Point | null => {
    if (!preview) return null;
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    return {
      x: Math.max(0, Math.min(preview.width, Math.round(((event.clientX - rect.left) / rect.width) * preview.width))),
      y: Math.max(0, Math.min(preview.height, Math.round(((event.clientY - rect.top) / rect.height) * preview.height))),
    };
  };

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    const point = sourcePoint(event);
    if (!point) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragStart(point);
    setDraft({ x: point.x, y: point.y, width: 0, height: 0 });
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!dragStart) return;
    const point = sourcePoint(event);
    if (!point) return;
    setDraft({
      x: Math.min(dragStart.x, point.x),
      y: Math.min(dragStart.y, point.y),
      width: Math.abs(point.x - dragStart.x),
      height: Math.abs(point.y - dragStart.y),
    });
  };

  const handlePointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDragStart(null);
  };

  const applyArea = () => {
    if (!preview || !draft) return;
    const width = Math.floor(draft.width / 2) * 2;
    const height = Math.floor(draft.height / 2) * 2;
    if (width < 64 || height < 64) {
      setPreviewError("Drag an area of at least 64 × 64 pixels.");
      return;
    }
    const crop = {
      x: Math.min(draft.x, preview.width - width),
      y: Math.min(draft.y, preview.height - height),
      width,
      height,
    };
    onCropChange(crop);
    closeAreaSelector();
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="inline-flex rounded-full border border-border bg-muted p-1 text-sm">
          <TabButton active={kind === "display"} onClick={() => setKind("display")}>
            <Monitor className="size-4" /> Entire display
          </TabButton>
          <TabButton active={kind === "window"} onClick={() => setKind("window")}>
            <AppWindow className="size-4" /> Application window
          </TabButton>
        </div>
        <button
          type="button"
          onClick={() => void refreshSources()}
          disabled={refreshing}
          className="inline-flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-60"
        >
          <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
          Refresh apps
        </button>
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {kind === "display"
          ? displays.map((display) => (
              <SourceCard
                key={display.id}
                title={display.name}
                subtitle={`${display.width} × ${display.height}${display.isPrimary ? " · Primary" : ""}`}
                selected={value?.kind === "display" && value.id === display.id}
                onClick={() => chooseTarget({ kind: "display", id: display.id })}
                iconKind="display"
              />
            ))
          : windows.map((window) => (
              <SourceCard
                key={window.id}
                title={window.name}
                subtitle={`${window.width} × ${window.height}`}
                selected={value?.kind === "window" && value.id === window.id}
                onClick={() => chooseTarget({ kind: "window", id: window.id })}
                iconKind="window"
              />
            ))}
      </div>

      {kind === "window" && windows.length === 0 && !refreshing && (
        <p className="rounded-xl border border-dashed border-border p-4 text-sm text-muted-foreground">
          No open, visible application windows are currently available. Open the app, then press Refresh apps.
        </p>
      )}

      {value && selectedSource && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-muted/40 p-3">
          <div>
            <p className="text-sm font-medium text-foreground">
              {cropRegion ? "Selected recording area" : "Full source will be recorded"}
            </p>
            <p className="text-xs text-muted-foreground">
              {cropRegion
                ? `${cropRegion.width} × ${cropRegion.height} at ${cropRegion.x}, ${cropRegion.y}`
                : `${selectedSource.width} × ${selectedSource.height}`}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {cropRegion && (
              <Button type="button" variant="outline" size="sm" onClick={() => onCropChange(null)}>
                Record full source
              </Button>
            )}
            <Button type="button" variant="outline" size="sm" onClick={() => void openAreaSelector()} disabled={previewLoading}>
              {previewLoading ? <Loader2 className="size-4 animate-spin" /> : <Crop className="size-4" />}
              Select area
            </Button>
          </div>
        </div>
      )}

      {previewError && !preview && <p className="text-xs text-destructive">{previewError}</p>}

      {preview && (
        <div className="rounded-2xl border border-border bg-surface p-4 shadow-[var(--shadow-soft)]">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-foreground">Drag over the area to record</h3>
              <p className="text-xs text-muted-foreground">The recorded MP4 will contain only the highlighted rectangle.</p>
            </div>
            <button type="button" onClick={closeAreaSelector} className="rounded-full p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground">
              <X className="size-4" />
            </button>
          </div>

          <div
            className="relative mx-auto max-h-[60vh] w-full cursor-crosshair touch-none select-none overflow-hidden rounded-xl border border-border bg-black"
            style={{ aspectRatio: `${preview.width} / ${preview.height}` }}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
          >
            <img src={preview.dataUrl} alt="Selected source preview" draggable={false} className="pointer-events-none size-full object-fill" />
            {draft && draft.width > 0 && draft.height > 0 && (
              <div
                className="pointer-events-none absolute border-2 border-white shadow-[0_0_0_9999px_rgba(0,0,0,0.55)]"
                style={{
                  left: `${(draft.x / preview.width) * 100}%`,
                  top: `${(draft.y / preview.height) * 100}%`,
                  width: `${(draft.width / preview.width) * 100}%`,
                  height: `${(draft.height / preview.height) * 100}%`,
                }}
              >
                <span className="absolute left-2 top-2 rounded bg-black/70 px-2 py-1 text-[11px] font-medium text-white">
                  {Math.floor(draft.width / 2) * 2} × {Math.floor(draft.height / 2) * 2}
                </span>
              </div>
            )}
          </div>

          {previewError && <p className="mt-3 text-xs text-destructive">{previewError}</p>}
          <div className="mt-4 flex flex-wrap justify-end gap-2">
            <Button type="button" variant="outline" onClick={() => { onCropChange(null); closeAreaSelector(); }}>
              Use full source
            </Button>
            <Button type="button" onClick={applyArea} disabled={!draft || draft.width < 64 || draft.height < 64}>
              Use selected area
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-2 rounded-full px-4 py-1.5 text-sm font-medium transition-colors",
        active ? "bg-surface text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function SourceCard({
  title,
  subtitle,
  selected,
  onClick,
  iconKind,
}: {
  title: string;
  subtitle: string;
  selected: boolean;
  onClick: () => void;
  iconKind: "display" | "window";
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group flex items-center gap-3 rounded-xl border border-border bg-surface p-3 text-left transition-all",
        "hover:border-border-strong hover:shadow-[var(--shadow-soft)]",
        selected && "border-foreground/40 ring-2 ring-ring/40",
      )}
    >
      <div
        className={cn(
          "flex aspect-video w-24 shrink-0 items-center justify-center rounded-lg border border-border bg-muted",
          selected && "bg-accent/15",
        )}
      >
        {iconKind === "display" ? <Monitor className="size-6 text-muted-foreground" /> : <AppWindow className="size-6 text-muted-foreground" />}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-foreground">{title}</div>
        <div className="truncate text-xs text-muted-foreground">{subtitle}</div>
      </div>
    </button>
  );
}
