import { useEffect, useState } from "react";
import { Mic, MicOff } from "lucide-react";
import * as desktop from "@/services/desktop";
import type { MicrophoneInfo } from "@/types/recorder";
import { Switch } from "@/components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { LevelMeter } from "./LevelMeter";

interface Props {
  enabled: boolean;
  onEnabledChange: (v: boolean) => void;
  micId: string | null;
  onMicChange: (id: string | null) => void;
  previewActive?: boolean;
}

export function MicSelector({ enabled, onEnabledChange, micId, onMicChange, previewActive = true }: Props) {
  const [mics, setMics] = useState<MicrophoneInfo[]>([]);

  useEffect(() => {
    desktop.listMicrophones().then((list) => {
      setMics(list);
      if (!micId && list.length) onMicChange(list.find((m) => m.isDefault)?.id ?? list[0].id);
    });
  }, [micId, onMicChange]);

  return (
    <SectionShell
      icon={enabled ? <Mic className="size-4" /> : <MicOff className="size-4" />}
      title="Microphone"
      trailing={<Switch checked={enabled} onCheckedChange={onEnabledChange} />}
    >
      <div className="space-y-3">
        <Select value={micId ?? ""} onValueChange={(v) => onMicChange(v)} disabled={!enabled}>
          <SelectTrigger>
            <SelectValue placeholder="Select microphone" />
          </SelectTrigger>
          <SelectContent>
            {mics.map((m) => (
              <SelectItem key={m.id} value={m.id}>
                {m.name}{m.isDefault ? " · Default" : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <LevelMeter active={enabled && previewActive} micId={enabled && previewActive ? micId : null} />
      </div>
    </SectionShell>
  );
}

export function SectionShell({
  icon,
  title,
  trailing,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  trailing?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="surface-card p-5">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="flex size-7 items-center justify-center rounded-md bg-muted text-muted-foreground">{icon}</span>
          <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        </div>
        {trailing}
      </div>
      {children}
    </div>
  );
}
