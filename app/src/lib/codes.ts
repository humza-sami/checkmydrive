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

/** Translate an engine code (issue.*, progress.*, suitability.*, error.*) with a safe fallback. */
export function tCode(t: TFunction, code: string, params?: Params, fallbackKey?: string): string {
  const p = prepParams(params);
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
