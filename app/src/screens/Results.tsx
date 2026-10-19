import { useTranslation } from "react-i18next";
import { HealthTile, Properties } from "../components/DriveOverview";
import { XpIcon, type IconName } from "../components/XpIcon";
import { metricLabel, suitabilityText, tCode } from "../lib/codes";
import { formatBytes, scoreTone, SEVERITY_ORDER, verdictTone } from "../lib/format";
import { summaryText } from "../lib/report";
import type { Status, Suitability, TestReport } from "../lib/types";

const LEVEL_ICON: Record<Suitability["level"], IconName> = { good: "ok", caution: "warn", no: "error" };
const STATUS_ICON: Record<Status, IconName> = { pass: "ok", warn: "warn", fail: "error", skipped: "skipped" };
const SEV_ICON = { critical: "error", warn: "warn", info: "info" } as const;

export function Results({ report: r }: { report: TestReport; onAnother?: () => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const tone = scoreTone(r.score);
  const vTone = verdictTone(r.verdict);
  const issues = [...r.issues].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);
  const sf = r.suitability;
  const skippedCapacity = r.tests.find((x) => x.id === "capacity_check")?.status === "skipped";
  const word = (l: Suitability["level"]) => (l === "good" ? t("results.can.yes") : l === "caution" ? t("results.can.maybe") : t("results.can.no"));

  const can: { key: string; q: string; level: Suitability["level"]; answer: string; why: string }[] = [
    { key: "os", q: t("results.can.os"), level: sf.os_install.level, answer: word(sf.os_install.level), why: suitabilityText(t, "os_install", sf.os_install) },
    { key: "storage", q: t("results.can.storage"), level: sf.long_term_storage.level, answer: word(sf.long_term_storage.level), why: suitabilityText(t, "long_term_storage", sf.long_term_storage) },
    { key: "daily", q: t("results.can.daily"), level: sf.daily_use.level, answer: word(sf.daily_use.level), why: suitabilityText(t, "daily_use", sf.daily_use) },
    sf.safe_fill_bytes > 0
      ? { key: "fill", q: t("results.can.fill"), level: "good", answer: formatBytes(sf.safe_fill_bytes, lang), why: t("results.can.fillYes", { size: formatBytes(sf.safe_fill_bytes, lang), total: formatBytes(r.drive.capacity_bytes, lang) }) }
      : { key: "fill", q: t("results.can.fill"), level: "no", answer: t("results.can.no"), why: t("results.can.fillNo") },
  ];

  return (
    <section aria-labelledby="res-title" className="results">
      <h1 id="res-title" className="sr-only">{t("results.title")}</h1>

      <div className="top-row">
        <fieldset className="health-box" data-testid="hero">
          <legend>{t("health.title")}</legend>
          <div className="health-inner">
            <HealthTile tone={tone} score={r.score} />
            <div className="health-text">
              <div className={`verdict ${vTone}`} data-testid="verdict">
                <XpIcon name={vTone === "good" ? "ok" : vTone === "caution" ? "warn" : "error"} size={16} />
                <span>{t(`results.verdict.${r.verdict}`)}</span>
              </div>
              <p className="summary">{summaryText(t, r)}</p>
              <p className="meta">
                {t(`results.mode.${r.mode}`)}<br />
                {t("results.tested", { date: new Date(r.started_at).toLocaleString(lang) })}
              </p>
            </div>
          </div>
        </fieldset>
        <fieldset className="props-box">
          <legend>{t("props.title")}</legend>
          <Properties drive={r.drive} report={r} />
          <p className="foot">{r.age.power_on_hours != null ? t("results.age.note") : t("results.age.unknown")}</p>
        </fieldset>
      </div>

      <fieldset>
        <legend>{t("results.can.title")}</legend>
        <table className="can-table">
          <tbody>
            {can.map((c) => (
              <tr key={c.key} className={c.level}>
                <td className="ico"><XpIcon name={LEVEL_ICON[c.level]} size={16} /></td>
                <th scope="row">{c.q}</th>
                <td className={`ans ${c.level}`}>{c.answer}</td>
                <td className="why">{c.why}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {skippedCapacity && <p className="foot"><XpIcon name="info" size={12} className="inl" /> {t("results.skippedNote")}</p>}
      </fieldset>

      <fieldset>
        <legend>{t("results.issues.title")}</legend>
        {issues.length === 0 ? (
          <div className="listbox none"><XpIcon name="ok" size={16} /> {t("results.issues.none")}</div>
        ) : (
          <ul className="listbox issue-list" data-testid="issues" tabIndex={0} aria-label={t("results.issues.title")}>
            {issues.map((i, idx) => (
              <li key={idx} className={`issue ${i.severity}`}>
                <XpIcon name={SEV_ICON[i.severity]} size={16} />
                <span className="sev">{t(`results.issues.${i.severity}`)}</span>
                <span className="txt">{tCode(t, i.code, i.params)}</span>
              </li>
            ))}
          </ul>
        )}
      </fieldset>

      <fieldset>
        <legend>{t("results.testResults")}</legend>
        <div className="listview-wrap">
          <table className="listview" data-testid="matrix">
            <thead>
              <tr>
                <th scope="col">{t("results.matrix.test")}</th>
                <th scope="col">{t("results.matrix.status")}</th>
                <th scope="col" className="num">{t("results.matrix.score")}</th>
                <th scope="col">{t("results.matrix.details")}</th>
              </tr>
            </thead>
            <tbody>
              {r.tests.map((x) => (
                <tr key={x.id}>
                  <th scope="row">{t(`tests.${x.id}.name`)}</th>
                  <td><span className="st"><XpIcon name={STATUS_ICON[x.status]} size={16} />{t(`status.${x.status}`)}</span></td>
                  <td className="num">{x.score ?? "—"}</td>
                  <td className="details">
                    {Object.keys(x.metrics).length === 0
                      ? <span className="dim">{t("results.matrix.none")}</span>
                      : Object.entries(x.metrics).map(([k, v]) => <span key={k} className="metric">{metricLabel(t, k)}: <b>{String(v)}</b></span>)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </fieldset>
    </section>
  );
}
