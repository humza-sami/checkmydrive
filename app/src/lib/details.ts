import type { TFunction } from "i18next";
import type { TestResult } from "./types";

type Metrics = TestResult["metrics"];

/** "0.27" / "17.5" / "912": never rounds a real speed down to 0. */
export function formatSpeed(mbps: number, locale?: string): string {
  const digits = mbps >= 100 ? 0 : mbps >= 1 ? 1 : 2;
  return new Intl.NumberFormat(locale, { maximumFractionDigits: digits, minimumFractionDigits: 0 }).format(mbps);
}

/** 12599.73 ms -> "12.6 s", 129.2 -> "129 ms". Units are universal symbols. */
export function formatLatency(ms: number, locale?: string): string {
  const nf = (v: number, d: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: d }).format(v);
  return ms >= 1000 ? `${nf(ms / 1000, 1)} s` : `${nf(ms, ms >= 100 ? 0 : 1)} ms`;
}

const num = (v: unknown): number | null => (typeof v === "number" && isFinite(v) ? v : null);
const int = (v: number, locale?: string) => new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(v);

export interface Highlight { label: string; value: string }

/** The few friendly, translated, non-null facts shown in the Test Results table. Raw metrics are never dumped. */
export function testHighlights(t: TFunction, test: TestResult, locale?: string): Highlight[] {
  const m: Metrics = test.metrics;
  const out: Highlight[] = [];
  const add = (labelKey: string, value: string | null) => { if (value != null) out.push({ label: t(labelKey) as string, value }); };
  const n = (k: string) => num(m[k]);
  const count = (labelKey: string, k: string, onlyIfPositive = false) => {
    const v = n(k);
    if (v != null && (!onlyIfPositive || v > 0)) add(labelKey, int(v, locale));
  };

  switch (test.id) {
    case "drive_info": {
      if (typeof m.bus === "string" && m.bus) add("metric.interface", m.bus.toUpperCase());
      break;
    }
    case "smart_health": {
      count("metric.reallocated_sectors", "reallocated_sectors");
      const pending = n("pending_sectors"), unc = n("uncorrectable_sectors");
      if (pending != null && unc != null && pending === unc) count("metric.uncorrectable_sectors", "uncorrectable_sectors");
      else { count("metric.pending_sectors", "pending_sectors"); count("metric.uncorrectable_sectors", "uncorrectable_sectors"); }
      count("metric.media_errors", "media_errors", true);
      count("detail.crc_errors", "crc_errors", true);
      const wear = n("wear_used_percent");
      if (wear != null) add("metric.percent_used", int(wear, locale));
      count("metric.temperature_c", "temperature_c");
      break;
    }
    case "selftest_short": {
      if (typeof m.final_state === "string") add("metric.result", t(`detail.selftest.${m.final_state}`, { defaultValue: m.final_state }) as string);
      break;
    }
    case "surface_read": {
      count("metric.read_errors", "read_errors");
      const lat = n("latency_ms_max");
      if (lat != null) add("detail.slowest", formatLatency(lat, locale));
      count("metric.slow_samples", "slow_samples", true);
      const mbps = n("read_mbps_avg");
      if (mbps != null) add("metric.read_mbps_avg", formatSpeed(mbps, locale));
      const done = n("samples_total"), planned = n("samples_planned");
      if (done != null && planned != null) add("metric.sectors_checked", t("detail.of", { done: int(done, locale), total: int(planned, locale) }) as string);
      break;
    }
    case "write_verify": {
      const w = n("write_mbps"), r = n("read_mbps");
      if (w != null) add("metric.write_mbps", formatSpeed(w, locale));
      if (r != null) add("detail.read_mbps", formatSpeed(r, locale));
      const written = n("bytes_written");
      if (written != null && written > 0) add("metric.written_mb", int(written / 1e6, locale));
      count("metric.mismatches", "verify_mismatches", true);
      count("metric.io_errors", "write_errors", true);
      break;
    }
    case "capacity_check": {
      count("metric.sectors_checked", "sectors_tested");
      count("metric.mismatches", "mismatches", true);
      count("metric.io_errors", "io_errors", true);
      const real = n("estimated_real_bytes");
      if (real != null) add("metric.real_capacity_gb", int(real / 1e9, locale));
      break;
    }
    case "stability": {
      count("detail.new_bad_areas", "reallocated_delta", true);
      count("detail.new_unreadable", "pending_delta", true);
      count("detail.new_crc", "crc_error_delta", true);
      count("metric.io_errors", "io_errors", true);
      count("detail.timeouts", "timeouts", true);
      count("metric.disconnects", "disconnect_events", true);
      if (out.length === 0 && m.smart_compared !== undefined) out.push({ label: t("detail.noProblems") as string, value: "" });
      break;
    }
  }
  return out;
}

/** Every metric, for the "Details…" dialog and the saved report: key + value as text ("—" for null). */
export function rawMetrics(test: TestResult): [string, string][] {
  return Object.entries(test.metrics).map(([k, v]) => [k, v === null || v === undefined || v === "" ? "—" : String(v)]);
}
