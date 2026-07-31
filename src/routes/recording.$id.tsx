import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, FolderOpen, Pencil, Trash2, Plus, Film } from "lucide-react";
import { AppShell } from "@/components/AppShell";
import * as desktop from "@/services/desktop";
import type { RecordingOutput } from "@/types/recorder";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatBytes, formatElapsed } from "@/hooks/useRecorder";

export const Route = createFileRoute("/recording/$id")({
  head: ({ params }) => ({
    meta: [
      { title: `Recording — Recorder` },
      { name: "description", content: "Preview, rename, and manage this recording." },
      { property: "og:title", content: "Recording — Recorder" },
      { property: "og:description", content: `Recording ${params.id}` },
    ],
  }),
  component: RecordingPreviewPage,
});

function RecordingPreviewPage() {
  const { id } = Route.useParams();
  const navigate = useNavigate();
  const [rec, setRec] = useState<RecordingOutput | null>(null);
  const [notFound, setNotFound] = useState(false);
  const [editing, setEditing] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [videoError, setVideoError] = useState(false);

  useEffect(() => {
    desktop.getRecording(id).then((r) => {
      if (r) {
        setRec(r);
        setTitleDraft(r.title);
      } else {
        setNotFound(true);
      }
    });
  }, [id]);

  const videoUrl = useMemo(() => (rec ? desktop.localFileUrl(rec.filePath) : null), [rec]);

  if (notFound) {
    return (
      <AppShell>
        <div className="surface-card p-8 text-center">
          <p className="text-sm">Recording not found.</p>
          <Link to="/library" className="mt-3 inline-block text-sm underline">
            Back to library
          </Link>
        </div>
      </AppShell>
    );
  }

  if (!rec) {
    return (
      <AppShell>
        <div className="surface-card animate-pulse p-8">Loading…</div>
      </AppShell>
    );
  }

  const saveTitle = async () => {
    const updated = await desktop.renameRecording(rec.id, titleDraft.trim() || rec.title);
    setRec(updated);
    setEditing(false);
  };

  const onDelete = async () => {
    await desktop.deleteRecording(rec.id);
    navigate({ to: "/library" });
  };

  return (
    <AppShell>
      <Link
        to="/library"
        className="mb-6 inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="size-4" /> Library
      </Link>

      <div className="surface-card overflow-hidden">
        <div className="relative flex aspect-video items-center justify-center bg-black">
          {videoUrl && !videoError ? (
            <video
              className="h-full w-full object-contain"
              src={videoUrl}
              poster={rec.thumbnailDataUrl ?? undefined}
              controls
              playsInline
              preload="auto"
              onError={() => setVideoError(true)}
            />
          ) : (
            <div className="flex flex-col items-center gap-2 px-6 text-center text-muted-foreground">
              <Film className="size-10 opacity-50" />
              <p className="text-sm font-medium">Video file could not be opened</p>
              <p className="max-w-lg text-xs">
                {rec.fileSizeBytes === 0
                  ? "This is an older placeholder recording created before the real MP4 pipeline was enabled. Create a new recording."
                  : "The recording exists, but the desktop player could not load it. Use Open file location to inspect the MP4."}
              </p>
            </div>
          )}
        </div>

        <div className="border-t border-border p-6">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 flex-1">
              {editing ? (
                <div className="flex items-center gap-2">
                  <Input
                    value={titleDraft}
                    onChange={(e) => setTitleDraft(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && saveTitle()}
                    autoFocus
                  />
                  <Button size="sm" onClick={saveTitle}>
                    Save
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setEditing(false);
                      setTitleDraft(rec.title);
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className="flex items-center gap-2">
                  <h1 className="truncate text-xl font-semibold">{rec.title}</h1>
                  <button
                    onClick={() => setEditing(true)}
                    className="text-muted-foreground hover:text-foreground"
                    aria-label="Rename"
                  >
                    <Pencil className="size-3.5" />
                  </button>
                </div>
              )}
              <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <span>{formatElapsed(rec.durationMs)}</span>
                <span>·</span>
                <span>
                  {rec.width} × {rec.height}
                </span>
                <span>·</span>
                <span>{formatBytes(rec.fileSizeBytes)}</span>
                <span>·</span>
                <span>{new Date(rec.createdAt).toLocaleString()}</span>
              </div>
              <div className="mt-2 truncate font-mono text-xs text-muted-foreground/80">
                {rec.filePath}
              </div>
            </div>
          </div>

          <div className="mt-6 flex flex-wrap gap-2">
            <Button
              variant="outline"
              onClick={() => desktop.openRecordingLocation(rec.id)}
              className="gap-1.5"
            >
              <FolderOpen className="size-4" /> Open file location
            </Button>
            <Link to="/">
              <Button variant="outline" className="gap-1.5">
                <Plus className="size-4" /> Record another
              </Button>
            </Link>
            <div className="flex-1" />
            <Button
              variant="ghost"
              onClick={onDelete}
              className="gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
            >
              <Trash2 className="size-4" /> Delete
            </Button>
          </div>
        </div>
      </div>
    </AppShell>
  );
}
