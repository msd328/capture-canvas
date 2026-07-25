interface Props {
  value: number;
}

export function Countdown({ value }: Props) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-xl">
      <div className="flex size-48 items-center justify-center rounded-full border border-border-strong bg-surface shadow-[var(--shadow-elevated)]">
        <span
          key={value}
          className="text-8xl font-semibold tabular-nums text-foreground [animation:pop_800ms_ease-out]"
        >
          {value}
        </span>
      </div>
      <style>{`
        @keyframes pop {
          0% { transform: scale(0.6); opacity: 0; }
          40% { transform: scale(1.05); opacity: 1; }
          100% { transform: scale(1); opacity: 1; }
        }
      `}</style>
    </div>
  );
}
