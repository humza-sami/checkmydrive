import type { TFunction } from "i18next";
import { formatBytes } from "./format";
import type { Params, Suitability } from "./types";

/** Params ending in `_bytes` are shown as friendly sizes ("32 GB"). */
export function prepParams(params?: Params): Params | undefined {
  if (!params) return params;
  const out: Params = { ...params };
  for (const [k, v] of Object.entries(params)) {
    if (k.endsWith("_bytes") && typeof v === "number") out[k] = formatBytes(v);
  }
  return out;
}

/**
 * i18next only pluralizes on the `count` option. Most engine codes already send their quantity as
 * `count`; for the few that name it differently, this table says which numeric param is the quantity
 * the sentence must agree with. Codes not listed (and without `count`) are never pluralized.
 * The engine's JSON contract is unchanged: `count` is added here, in the GUI, and the original
 * param (e.g. `years`) stays available to the translation text.
 */
export const COUNT_PARAM: Record<string, string> = {
  "issue.age.high_hours": "years",
  "issue.age.moderate_hours": "years",
  "issue.selftest.timeout": "seconds",
  "progress.selftest_short.running": "seconds", // the `_seconds` context variant
};

/** Add `count` (the quantity the text agrees with) to engine params when the code needs it. */
export function withCount(code: string, params?: Params): Params | undefined {
  if (!params || typeof params.count === "number") return params;
  const name = COUNT_PARAM[code];
  const v = name ? params[name] : undefined;
  return typeof v === "number" ? { ...params, count: v } : params;
}

/** Translate an engine code (issue.*, progress.*, suitability.*, error.*) with a safe fallback. */
export function tCode(t: TFunction, code: string, params?: Params, fallbackKey?: string): string {
  const p = prepParams(withCount(code, params));
  const fallback = fallbackKey
    ? t(fallbackKey, p as never)
    : t(`${code.split(".")[0]}.unknown`, { ...p, code } as never);
  // The engine reports `seconds` instead of `percent` when the drive gives no progress figure.
  const context = code === "progress.selftest_short.running" && p && p.percent === undefined && p.seconds !== undefined ? "seconds" : undefined;
  return t(code, { ...p, context, defaultValue: fallback } as never) as unknown as string;
}
export function suitabilityText(t: TFunction, kind: string, s: Suitability): string {
  return tCode(t, s.reason_code, s.params, `suitability.fallback.${kind}.${s.level}`);
}

export function metricLabel(t: TFunction, key: string): string {
  const human = key.replace(/_/g, " ");
  return t(`metric.${key}`, { defaultValue: human }) as unknown as string;
}
