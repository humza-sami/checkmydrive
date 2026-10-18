import type { Verdict } from "./types";

/** Drive makers use decimal units (1 GB = 1,000,000,000 bytes); so do we. */
export function formatBytes(bytes: number | null | undefined, locale?: string): string {
  if (bytes == null || !isFinite(bytes) || bytes < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let v = bytes;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  const digits = i === 0 || v >= 100 ? 0 : v >= 10 ? 1 : 2;
  const num = new Intl.NumberFormat(locale, {
    maximumFractionDigits: digits,
    minimumFractionDigits: 0,
  }).format(v);
  return `${num} ${units[i]}`;
}

export const HOURS_PER_YEAR = 24 * 365;

/** Power-on hours to years, one decimal. Null in, null out. */
export function hoursToYears(hours: number | null | undefined): number | null {
  if (hours == null || !isFinite(hours) || hours < 0) return null;
  return Math.round((hours / HOURS_PER_YEAR) * 10) / 10;
}

/** 5 -> "0:05", 125 -> "2:05", 3700 -> "1:01:40" */
export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

export type ScoreTone = "good" | "caution" | "bad";

export function scoreTone(score: number): ScoreTone {
  return score >= 80 ? "good" : score >= 50 ? "caution" : "bad";
}

export function verdictFromScore(score: number): Verdict {
  return score >= 80 ? "buy" : score >= 50 ? "caution" : "avoid";
}

export function verdictTone(v: Verdict): ScoreTone {
  return v === "buy" ? "good" : v === "caution" ? "caution" : "bad";
}

/** Sort key: critical first. */
export const SEVERITY_ORDER = { critical: 0, warn: 1, info: 2 } as const;

/** "2024-05" / "2024-W18" -> localized text. Falls back to the raw string. */
export function formatManufactureDate(s: string | null | undefined, locale?: string): string | null {
  if (!s) return null;
  const m = /^(\d{4})-(\d{2})$/.exec(s);
  if (m) {
    const d = new Date(Date.UTC(+m[1], +m[2] - 1, 1));
    return new Intl.DateTimeFormat(locale, { year: "numeric", month: "long", timeZone: "UTC" }).format(d);
  }
  const w = /^(\d{4})-W(\d{2})$/.exec(s);
  if (w) return `${w[1]} · W${w[2]}`;
  return s;
}
