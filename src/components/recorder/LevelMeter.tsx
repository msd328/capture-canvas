import { useEffect, useRef, useState } from "react";
import * as desktop from "@/services/desktop";
import { cn } from "@/lib/utils";

interface Props {
  active: boolean;
  micId: string | null;
}

export function LevelMeter({ active, micId }: Props) {
  const [level, setLevel] = useState(0);
  const smoothedRef = useRef(0);

  useEffect(() => {
    if (!active || !micId) {
      setLevel(0);
      return;
    }
    const unsub = desktop.subscribeMicLevel(micId, (l) => {
      smoothedRef.current = smoothedRef.current * 0.6 + l * 0.4;
      setLevel(smoothedRef.current);
    });
    return unsub;
  }, [active, micId]);

  const bars = 20;
  return (
    <div className="flex h-8 items-end gap-[3px]" aria-label="microphone level">
      {Array.from({ length: bars }).map((_, i) => {
        const threshold = (i + 1) / bars;
        const on = level >= threshold - 0.05;
        return (
          <span
            key={i}
            className={cn(
              "flex-1 rounded-sm bg-border transition-colors",
              on && i < bars * 0.7 && "bg-success",
              on && i >= bars * 0.7 && i < bars * 0.9 && "bg-accent",
              on && i >= bars * 0.9 && "bg-destructive",
            )}
            style={{ height: `${20 + (i / bars) * 80}%` }}
          />
        );
      })}
    </div>
  );
}
