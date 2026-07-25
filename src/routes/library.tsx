import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Film, Plus } from "lucide-react";
import { AppShell } from "@/components/AppShell";
import * as desktop from "@/services/desktop";
import type { RecordingOutput } from "@/types/recorder";
import { Button } from "@/components/ui/button";
import { formatBytes, formatElapsed } from "@/hooks/useRecorder";

export const Route = createFileRoute("/library")({
  head: () => ({
    meta: [
      { title: "Library — Recorder" },
      { name: "description", content: "Browse and manage your saved screen recordings." },
      { property: "og:title", content: "Library — Recorder" },
      { property: "og:description", content: "Browse and manage your saved screen recordings." },
    ],
  }),
  component: LibraryPage,
});

function LibraryPage() {
  const [items, setItems] = useState<RecordingOutput[]>([]);

  useEffect(() => {
    desktop.getRecordings().then(setItems);
  }, []);

  return (
    <AppShell>
      <div className="mb-8 flex items-end justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Library</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            {items.length} recording{items.length === 1 ? "" : "s"} saved locally.
          </p>
        </div>
        <Link to="/">
          <Button className="gap-1.5">
            <Plus className="size-4" /> New recording
          </Button>
        </Link>
      </div>

      {items.length === 0 ? (
        <div className="surface-card flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <Film className="size-5" />
          </div>
          <p className="text-sm font-medium">No recordings yet</p>
          <p className="max-w-sm text-xs text-muted-foreground">
            Recordings you capture will appear here. They're stored locally on your Mac or PC.
          </p>
          <Link to="/" className="mt-2">
            <Button variant="outline" size="sm">
              Start recording
            </Button>
          </Link>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {items.map((r) => (
            <Link
              key={r.id}
              to="/recording/$id"
              params={{ id: r.id }}
              className="group overflow-hidden rounded-xl border border-border bg-surface transition-all hover:border-border-strong hover:shadow-[var(--shadow-soft)]"
            >
              <div className="flex aspect-video items-center justify-center bg-[var(--gradient-surface)] text-muted-foreground">
                <Film className="size-8 opacity-40" />
              </div>
              <div className="p-4">
                <div className="truncate text-sm font-medium">{r.title}</div>
                <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                  <span>{formatElapsed(r.durationMs)}</span>
                  <span>·</span>
                  <span>{formatBytes(r.fileSizeBytes)}</span>
                  <span>·</span>
                  <span>{new Date(r.createdAt).toLocaleDateString()}</span>
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </AppShell>
  );
}
