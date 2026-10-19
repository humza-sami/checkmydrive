/** XP green block progress bar (segmented chunks). Static: no animation. */
export function ProgressBar({ value, label }: { value: number; label: string }) {
  const v = Math.max(0, Math.min(100, value));
  return (
    <div className="xp-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v)} aria-label={label}>
      <div className="xp-progress-fill" style={{ width: `${v}%` }} />
    </div>
  );
}
