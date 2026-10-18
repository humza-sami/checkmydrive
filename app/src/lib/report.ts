import type { TFunction } from "i18next";
import type { TestReport } from "./types";
import { formatBytes, formatManufactureDate, hoursToYears } from "./format";
import { metricLabel, suitabilityText, tCode } from "./codes";
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./engine";

const esc = (s: unknown) =>
  String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

export function summaryKey(r: TestReport): string {
  if (r.issues.some((i) => i.code === "issue.capacity_check.fake_capacity")) return "results.summary.fake_capacity";
  if (r.verdict === "avoid" && r.issues.some((i) => i.code === "issue.smart.overall_failed" || i.code === "issue.stability.device_disconnected" || i.code === "issue.smart.pending_sectors"))
    return "results.summary.failing";
  return `results.summary.${r.verdict}`;
}

export function summaryText(t: TFunction, r: TestReport): string {
  const fake = r.issues.find((i) => i.code === "issue.capacity_check.fake_capacity");
  return t(summaryKey(r), {
    model: r.drive.model ?? t("picker.unknownModel"),
    ...(fake?.params ?? {}),
    claimed: formatBytes(Number(fake?.params?.claimed_bytes ?? r.drive.capacity_bytes)),
  }) as string;
}

export function reportFileName(r: TestReport, ext: string): string {
  const base = (r.drive.model ?? "drive").replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase();
  return `checkmydrive-${base || "drive"}-${r.started_at.slice(0, 10)}.${ext}`;
}

/** Self-contained printable HTML report (no external assets). */
export function buildHtmlReport(r: TestReport, t: TFunction, locale: string, dir: "ltr" | "rtl"): string {
  const tone = r.score >= 80 ? "#15803d" : r.score >= 50 ? "#b45309" : "#b91c1c";
  const years = r.age.power_on_years ?? hoursToYears(r.age.power_on_hours);
  const rows = r.tests
    .map(
      (x) => `<tr><td>${esc(t(`tests.${x.id}.name`))}</td><td>${esc(t(`status.${x.status}`))}</td><td>${x.score ?? "—"}</td><td>${esc(
        Object.entries(x.metrics).map(([k, v]) => `${metricLabel(t, k)}: ${v}`).join("; ") || "—",
      )}</td></tr>`,
    )
    .join("");
  const issues = r.issues.length
    ? r.issues.map((i) => `<li class="${i.severity}"><b>${esc(t(`results.issues.${i.severity}`))}:</b> ${esc(tCode(t, i.code, i.params))}</li>`).join("")
    : `<li>${esc(t("results.issues.none"))}</li>`;
  const can = (label: string, kind: "os_install" | "long_term_storage" | "daily_use") =>
    `<li><b>${esc(label)}</b> ${esc(suitabilityText(t, kind, r.suitability[kind]))}</li>`;
  return `<!doctype html><html lang="${esc(locale)}" dir="${dir}"><head><meta charset="utf-8"><title>${esc(t("report.title"))} - ${esc(r.drive.model ?? "")}</title>
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:820px;margin:32px auto;padding:0 20px;color:#111}
h1{margin:0}.score{font-size:72px;font-weight:800;color:${tone};line-height:1}.verdict{font-size:24px;font-weight:700;color:${tone}}
table{border-collapse:collapse;width:100%;font-size:14px}td,th{border:1px solid #ccc;padding:6px 8px;text-align:start;vertical-align:top}
.critical{color:#b91c1c}.warn{color:#b45309}li{margin:6px 0}small{color:#555}</style></head><body>
<h1>${esc(t("report.title"))}</h1><p><small>${esc(r.drive.model ?? "")} · ${esc(formatBytes(r.drive.capacity_bytes, locale))} · ${esc(r.drive.serial ?? "")}<br>${esc(t("results.tested", { date: new Date(r.started_at).toLocaleString(locale) }))} · ${esc(t(`results.mode.${r.mode}`))}</small></p>
<div class="score">${r.score}<small> / 100</small></div><div class="verdict">${esc(t(`results.verdict.${r.verdict}`))}</div>
<p>${esc(summaryText(t, r))}</p>
<h2>${esc(t("results.can.title"))}</h2><ul>${can(t("results.can.os"), "os_install")}${can(t("results.can.storage"), "long_term_storage")}${can(t("results.can.daily"), "daily_use")}
<li><b>${esc(t("results.can.fill"))}</b> ${esc(r.suitability.safe_fill_bytes > 0 ? t("results.can.fillYes", { size: formatBytes(r.suitability.safe_fill_bytes, locale), total: formatBytes(r.drive.capacity_bytes, locale) }) : t("results.can.fillNo"))}</li></ul>
<h2>${esc(t("results.age.title"))}</h2><p>${esc(r.age.power_on_hours != null ? t("results.age.hours", { count: r.age.power_on_hours, hours: r.age.power_on_hours.toLocaleString(locale) }) + (years != null ? " (" + t(years < 1 ? "results.age.yearsLessThanOne" : "results.age.years", { count: years }) + ")" : "") : t("results.age.unknown"))}${r.age.manufacture_date ? "<br>" + esc(t("results.age.made", { date: formatManufactureDate(r.age.manufacture_date, locale) })) : ""}</p>
<h2>${esc(t("results.issues.title"))}</h2><ul>${issues}</ul>
<h2>${esc(t("results.matrix.title"))}</h2><table><thead><tr><th>${esc(t("results.matrix.test"))}</th><th>${esc(t("results.matrix.status"))}</th><th>${esc(t("results.matrix.score"))}</th><th>${esc(t("results.matrix.metrics"))}</th></tr></thead><tbody>${rows}</tbody></table>
<p><small>${esc(t("report.generatedBy"))} · ${esc(t("results.engine", { version: r.engine_version }))}<br>${esc(t("settings.disclaimer"))}</small></p></body></html>`;
}

/** Save text via the native dialog in Tauri, or a browser download otherwise. */
export async function saveTextFile(name: string, contents: string, mime: string): Promise<boolean> {
  if (inTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: name });
    if (!path) return false;
    await invoke("save_text_file", { path, contents });
    return true;
  }
  const url = URL.createObjectURL(new Blob([contents], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}
