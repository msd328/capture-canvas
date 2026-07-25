import { Monitor, AppWindow } from "lucide-react";
import { useEffect, useState } from "react";
import * as desktop from "@/services/desktop";
import type { CaptureKind, CaptureTarget, DisplayInfo, WindowInfo } from "@/types/recorder";
import { cn } from "@/lib/utils";

interface Props {
  value: CaptureTarget | null;
  onChange: (target: CaptureTarget) => void;
}

export function SourceSelector({ value, onChange }: Props) {
  const [kind, setKind] = useState<CaptureKind>("display");
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [windows, setWindows] = useState<WindowInfo[]>([]);

  useEffect(() => {
    desktop.listDisplays().then(setDisplays);
    desktop.listWindows().then(setWindows);
  }, []);

  return (
    <div className="space-y-4">
      <div className="inline-flex rounded-full border border-border bg-muted p-1 text-sm">
        <TabButton active={kind === "display"} onClick={() => setKind("display")}>
          <Monitor className="size-4" /> Entire display
        </TabButton>
        <TabButton active={kind === "window"} onClick={() => setKind("window")}>
          <AppWindow className="size-4" /> Application window
        </TabButton>
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {kind === "display"
          ? displays.map((d) => (
              <SourceCard
                key={d.id}
                title={d.name}
                subtitle={`${d.width} × ${d.height}${d.isPrimary ? " · Primary" : ""}`}
                selected={value?.kind === "display" && value.id === d.id}
                onClick={() => onChange({ kind: "display", id: d.id })}
                iconKind="display"
              />
            ))
          : windows.map((w) => (
              <SourceCard
                key={w.id}
                title={w.name}
                subtitle={w.appName}
                selected={value?.kind === "window" && value.id === w.id}
                onClick={() => onChange({ kind: "window", id: w.id })}
                iconKind="window"
              />
            ))}
      </div>
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
