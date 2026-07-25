import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { AppShell } from "@/components/AppShell";
import * as desktop from "@/services/desktop";
import type { CameraInfo, MicrophoneInfo, RecorderSettings } from "@/types/recorder";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { toast } from "sonner";

export const Route = createFileRoute("/settings")({
  head: () => ({
    meta: [
      { title: "Settings — Recorder" },
      { name: "description", content: "Configure default devices, output directory, and app preferences." },
      { property: "og:title", content: "Settings — Recorder" },
      { property: "og:description", content: "Configure Recorder preferences." },
    ],
  }),
  component: SettingsPage,
});

function SettingsPage() {
  const [settings, setSettings] = useState<RecorderSettings | null>(null);
  const [mics, setMics] = useState<MicrophoneInfo[]>([]);
  const [cams, setCams] = useState<CameraInfo[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    Promise.all([desktop.getSettings(), desktop.listMicrophones(), desktop.listCameras()]).then(([s, m, c]) => {
      setSettings(s);
      setMics(m);
      setCams(c);
    });
  }, []);

  if (!settings) {
    return (
      <AppShell>
        <div className="surface-card animate-pulse p-8">Loading…</div>
      </AppShell>
    );
  }

  const patch = (p: Partial<RecorderSettings>) => setSettings({ ...settings, ...p });

  const save = async () => {
    setSaving(true);
    try {
      const updated = await desktop.updateSettings(settings);
      setSettings(updated);
      toast.success("Settings saved");
    } finally {
      setSaving(false);
    }
  };

  return (
    <AppShell>
      <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
      <p className="mt-1 text-sm text-muted-foreground">Preferences apply to all new recordings.</p>

      <div className="mt-8 space-y-4">
        <Card title="Default microphone" description="Used as the initial selection on the recorder screen.">
          <Select
            value={settings.defaultMicrophoneId ?? ""}
            onValueChange={(v) => patch({ defaultMicrophoneId: v || null })}
          >
            <SelectTrigger><SelectValue placeholder="System default" /></SelectTrigger>
            <SelectContent>
              {mics.map((m) => <SelectItem key={m.id} value={m.id}>{m.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </Card>

        <Card title="Default camera" description="Used as the initial camera when webcam is turned on.">
          <Select
            value={settings.defaultCameraId ?? ""}
            onValueChange={(v) => patch({ defaultCameraId: v || null })}
          >
            <SelectTrigger><SelectValue placeholder="No default" /></SelectTrigger>
            <SelectContent>
              {cams.map((c) => <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </Card>

        <Card title="Frame rate" description="Target frames per second for video capture.">
          <Select value={String(settings.fps)} onValueChange={(v) => patch({ fps: Number(v) })}>
            <SelectTrigger className="w-40"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="24">24 fps</SelectItem>
              <SelectItem value="30">30 fps · recommended</SelectItem>
              <SelectItem value="60">60 fps</SelectItem>
            </SelectContent>
          </Select>
        </Card>

        <Card title="Output directory" description="Where MP4 files are saved on this device.">
          <div className="flex gap-2">
            <Input value={settings.outputDirectory} onChange={(e) => patch({ outputDirectory: e.target.value })} />
            <Button variant="outline" onClick={() => toast.info("Native folder picker available in desktop build.")}>Choose…</Button>
          </div>
        </Card>

        <Card title="Launch at startup" description="Open Recorder automatically when you sign in.">
          <Switch checked={settings.launchAtStartup} onCheckedChange={(v) => patch({ launchAtStartup: v })} />
        </Card>

        <Card title="Show camera bubble" description="Overlay a floating webcam bubble on the recorded video.">
          <Switch checked={settings.showCameraBubble} onCheckedChange={(v) => patch({ showCameraBubble: v })} />
        </Card>
      </div>

      <div className="mt-8 flex justify-end">
        <Button onClick={save} disabled={saving}>{saving ? "Saving…" : "Save changes"}</Button>
      </div>
    </AppShell>
  );
}

function Card({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="surface-card flex items-center justify-between gap-6 p-5">
      <div className="min-w-0 flex-1">
        <Label className="text-sm font-medium">{title}</Label>
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
