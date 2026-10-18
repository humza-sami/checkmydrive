import type { ScoreTone } from "../lib/format";

interface Props {
  value: number; // 0-100
  size?: number;
  stroke?: number;
  tone?: ScoreTone;
  className?: string;
  label: string;
  children?: React.ReactNode;
}

/** Circular progress / score ring. Purely visual; `label` is exposed to screen readers. */
export function Ring({ value, size = 240, stroke = 16, tone, className = "", label, children }: Props) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = Math.max(0, Math.min(100, value));
  return (
    <div className={`${className} ${tone ?? ""}`} style={{ width: size, height: size }} role="img" aria-label={label}>
      <svg viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
        <circle className="track" cx={size / 2} cy={size / 2} r={r} fill="none" strokeWidth={stroke} stroke="var(--ring-track)" />
        <circle
          className="bar glow"
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - v / 100)}
          stroke={tone === "good" ? "var(--good)" : tone === "caution" ? "var(--caution)" : tone === "bad" ? "var(--bad)" : "var(--accent)"}
          style={{ transition: "stroke-dashoffset .6s ease" }}
        />
      </svg>
      <div className="center num" aria-hidden="true">{children}</div>
    </div>
  );
}
