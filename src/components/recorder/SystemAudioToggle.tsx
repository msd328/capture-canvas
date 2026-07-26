import { Volume2, VolumeX, Info } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SectionShell } from "./MicSelector";

interface Props {
  enabled: boolean;
  onEnabledChange: (v: boolean) => void;
  supported: boolean;
  platformNote?: string;
}

export function SystemAudioToggle({ enabled, onEnabledChange, supported, platformNote }: Props) {
  return (
    <SectionShell
      icon={enabled ? <Volume2 className="size-4" /> : <VolumeX className="size-4" />}
      title="System audio"
      trailing={<Switch checked={enabled && supported} disabled={!supported} onCheckedChange={onEnabledChange} />}
    >
      {supported ? (
        <p className="text-xs text-muted-foreground">
          Windows audio output is available for native loopback capture. System audio will be mixed into the recording when the active recording backend can open that endpoint.
        </p>
      ) : (
        <div className="flex items-start gap-2 rounded-lg bg-muted p-3 text-xs text-muted-foreground">
          <Info className="mt-0.5 size-4 shrink-0" />
          <span>{platformNote ?? "System audio capture isn't available on this operating system yet."}</span>
        </div>
      )}
    </SectionShell>
  );
}
