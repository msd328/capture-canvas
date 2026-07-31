import { useEffect, useRef, useState } from "react";
import * as desktop from "@/services/desktop";
import { cn } from "@/lib/utils";

interface Props {
  active: boolean;
  micId: string | null;
}

function normalizeDeviceName(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

export function LevelMeter({ active, micId }: Props) {
  const [level, setLevel] = useState(0);
  const smoothedRef = useRef(0);

  useEffect(() => {
    if (!active || !micId || !desktop.isDesktop() || !navigator.mediaDevices?.getUserMedia) {
      smoothedRef.current = 0;
      setLevel(0);
      return;
    }

    let cancelled = false;
    let stream: MediaStream | null = null;
    let audioContext: AudioContext | null = null;
    let raf = 0;

    const start = async () => {
      try {
        const mics = await desktop.listMicrophones();
        const selected = mics.find((mic) => mic.id === micId);

        const permissionStream = await navigator.mediaDevices.getUserMedia({
          audio: true,
          video: false,
        });
        const devices = await navigator.mediaDevices.enumerateDevices();
        permissionStream.getTracks().forEach((track) => track.stop());

        const wanted = normalizeDeviceName(selected?.name ?? "");
        const browserMic = devices.find((device) => {
          if (device.kind !== "audioinput") return false;
          const label = normalizeDeviceName(device.label);
          return !!wanted && (label === wanted || label.includes(wanted) || wanted.includes(label));
        });

        stream = await navigator.mediaDevices.getUserMedia({
          audio: browserMic?.deviceId
            ? {
                deviceId: { exact: browserMic.deviceId },
                echoCancellation: false,
                noiseSuppression: false,
                autoGainControl: false,
              }
            : { echoCancellation: false, noiseSuppression: false, autoGainControl: false },
          video: false,
        });

        if (cancelled) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }

        audioContext = new AudioContext();
        const source = audioContext.createMediaStreamSource(stream);
        const analyser = audioContext.createAnalyser();
        analyser.fftSize = 512;
        analyser.smoothingTimeConstant = 0.65;
        source.connect(analyser);
        const samples = new Uint8Array(analyser.fftSize);

        const tick = () => {
          if (cancelled) return;
          analyser.getByteTimeDomainData(samples);
          let sum = 0;
          for (const value of samples) {
            const normalized = (value - 128) / 128;
            sum += normalized * normalized;
          }
          const rms = Math.sqrt(sum / samples.length);
          const scaled = Math.min(1, rms * 4.5);
          smoothedRef.current = smoothedRef.current * 0.65 + scaled * 0.35;
          setLevel(smoothedRef.current);
          raf = requestAnimationFrame(tick);
        };
        tick();
      } catch {
        if (!cancelled) {
          smoothedRef.current = 0;
          setLevel(0);
        }
      }
    };

    void start();
    return () => {
      cancelled = true;
      if (raf) cancelAnimationFrame(raf);
      stream?.getTracks().forEach((track) => track.stop());
      if (audioContext && audioContext.state !== "closed") void audioContext.close();
      smoothedRef.current = 0;
    };
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
