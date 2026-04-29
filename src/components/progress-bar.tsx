/**
 * Shared progress bar used for conversion and upload stages.
 */

interface ProgressBarProps {
  label: string;
  value: number;
  accent?: "emerald" | "cyan" | "amber";
  caption?: string;
}

export function ProgressBar({
  label,
  value,
  accent = "emerald",
  caption,
}: ProgressBarProps) {
  const percentage = Math.max(0, Math.min(100, value));
  const accentClass =
    accent === "cyan"
      ? "from-cyan-400 via-sky-400 to-cyan-300"
      : accent === "amber"
        ? "from-amber-400 via-orange-300 to-amber-200"
        : "from-emerald-400 via-lime-300 to-emerald-200";

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-xs text-slate-300/85">
        <span className="tracking-[0.12em] text-slate-400 uppercase">{label}</span>
        <span>{caption ?? `${percentage}%`}</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-white/6 ring-1 ring-white/6">
        <div
          className={`h-full rounded-full bg-gradient-to-r ${accentClass} transition-[width] duration-500 ease-out`}
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}
