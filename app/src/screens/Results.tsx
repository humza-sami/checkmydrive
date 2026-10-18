import artGood from "../assets/result-good.png";
import artCaution from "../assets/result-caution.png";
import artBad from "../assets/result-bad.png";
import { useState } from "react";
import {
  CalendarDays, Check, ChevronDown, CircleX, Clock, Download, Info, Printer, RotateCcw, ShieldAlert,
  ShieldCheck, ShieldX, TriangleAlert, FileJson, FileText,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Ring } from "../components/Ring";
import { metricLabel, suitabilityText, tCode } from "../lib/codes";
import {
  formatBytes, formatManufactureDate, hoursToYears, scoreTone, verdictTone, SEVERITY_ORDER,
} from "../lib/format";
import { buildHtmlReport, reportFileName, saveTextFile, summaryText } from "../lib/report";
import type { Suitability, TestReport } from "../lib/types";

function Can({ q, kind, s }: { q: string; kind: "os_install" | "long_term_storage" | "daily_use"; s: Suitability }) {
  const { t } = useTranslation();
  const Icon = s.level === "good" ? Check : s.level === "caution" ? TriangleAlert : CircleX;
  const word = s.level === "good" ? t("results.can.yes") : s.level === "caution" ? t("results.can.maybe") : t("results.can.no");
  return (
    <div className="card can-card">
      <div className="can-q">{q}</div>
      <div className={`can-a ${s.level}`}><Icon size={22} aria-hidden="true" /> {word}</div>
      <div className="can-why">{suitabilityText(t, kind, s)}</div>
    </div>
  );
}

const VERDICT_ART = { buy: artGood, caution: artCaution, avoid: artBad } as const;

export function Results({ report: r, onAnother }: { report: TestReport; onAnother: () => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [matrixOpen, setMatrixOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const tone = scoreTone(r.score);
  const vTone = verdictTone(r.verdict);
  const VIcon = r.verdict === "buy" ? ShieldCheck : r.verdict === "caution" ? ShieldAlert : ShieldX;
  const issues = [...r.issues].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);
  const years = r.age.power_on_years ?? hoursToYears(r.age.power_on_hours);
  const made = formatManufactureDate(r.age.manufacture_date, lang);
  const sf = r.suitability;
  const skippedCapacity = r.tests.find((x) => x.id === "capacity_check")?.status === "skipped";

  const save = async (kind: "json" | "html") => {
    setMenuOpen(false);
    try {
      const dir = document.documentElement.dir === "rtl" ? "rtl" : "ltr";
      const ok = kind === "json"
        ? await saveTextFile(reportFileName(r, "json"), JSON.stringify(r, null, 2), "application/json")
        : await saveTextFile(reportFileName(r, "html"), buildHtmlReport(r, t, lang, dir), "text/html");
      if (ok) setToast(t("results.saved"));
    } catch {
      setToast(t("results.saveFailed"));
    }
  };
  const print = () => {
    setMenuOpen(false);
    setMatrixOpen(true);
    setTimeout(() => window.print(), 150);
  };

  return (
    <section className="fade-in" aria-labelledby="res-title">
      <h1 id="res-title" className="sr-only">{t("results.title")}</h1>
      <div className="card hero" data-testid="hero">
        <div className="score-ring">
          <Ring value={r.score} size={220} stroke={18} tone={tone} className="" label={t("results.scoreLabel", { score: r.score })}>
            <span className={`big tone-${tone}`} data-testid="score">{r.score}</span>
            <span className="of">{t("results.scoreOutOf")}</span>
          </Ring>
        </div>
        <div>
          <div className={`verdict ${vTone}`} data-testid="verdict"><VIcon size={26} aria-hidden="true" /> {t(`results.verdict.${r.verdict}`)}</div>
          <p className="summary">{summaryText(t, r)}</p>
          <p className="meta">
            {r.drive.model ?? t("picker.unknownModel")} · {formatBytes(r.drive.capacity_bytes, lang)} · {t(`results.mode.${r.mode}`)}<br />
            {t("results.tested", { date: new Date(r.started_at).toLocaleString(lang) })}
          </p>
        </div>
        <img className="hero-art" src={VERDICT_ART[r.verdict]} alt="" />
      </div>

      <h2 className="section-title">{t("results.can.title")}</h2>
      <div className="cards3">
        <Can q={t("results.can.os")} kind="os_install" s={sf.os_install} />
        <Can q={t("results.can.storage")} kind="long_term_storage" s={sf.long_term_storage} />
        <Can q={t("results.can.daily")} kind="daily_use" s={sf.daily_use} />
        <div className="card can-card">
          <div className="can-q">{t("results.can.fill")}</div>
          {sf.safe_fill_bytes > 0 ? (
            <>
              <div className="can-a good"><Check size={22} aria-hidden="true" /> {formatBytes(sf.safe_fill_bytes, lang)}</div>
              <div className="can-why">{t("results.can.fillYes", { size: formatBytes(sf.safe_fill_bytes, lang), total: formatBytes(r.drive.capacity_bytes, lang) })}</div>
            </>
          ) : (
            <>
              <div className="can-a no"><CircleX size={22} aria-hidden="true" /> {t("results.can.no")}</div>
              <div className="can-why">{t("results.can.fillNo")}</div>
            </>
          )}
        </div>
      </div>
      {skippedCapacity && <p className="hint" style={{ marginTop: 12 }}><Info size={14} aria-hidden="true" style={{ verticalAlign: "-2px" }} /> {t("results.skippedNote")}</p>}

      <h2 className="section-title">{t("results.age.title")}</h2>
      <div className="card age-grid">
        <Clock size={36} aria-hidden="true" color="var(--accent)" />
        <div>
          {r.age.power_on_hours != null ? (
            <>
              <div className="age-big">
                {years != null && years < 1 ? t("results.age.yearsLessThanOne") : t("results.age.years", { count: years ?? 0 })}
              </div>
              <div>{t("results.age.hours", { count: r.age.power_on_hours, hours: r.age.power_on_hours.toLocaleString(lang) })}
                {r.age.power_cycles != null && <> · {t("results.age.cycles", { count: r.age.power_cycles })}</>}</div>
            </>
          ) : (
            <div className="age-big" style={{ fontSize: 20 }}>{t("results.age.unknown")}</div>
          )}
          {made && <div style={{ marginTop: 4 }}><CalendarDays size={16} aria-hidden="true" style={{ verticalAlign: "-3px" }} /> {t("results.age.made", { date: made })}</div>}
          <p className="hint" style={{ marginTop: 8 }}>{t("results.age.note")}</p>
        </div>
      </div>

      <h2 className="section-title">{t("results.issues.title")}</h2>
      {issues.length === 0 ? (
        <div className="card"><Check aria-hidden="true" color="var(--good)" style={{ verticalAlign: "-4px" }} /> {t("results.issues.none")}</div>
      ) : (
        <ul className="issue-list" data-testid="issues">
          {issues.map((i, idx) => {
            const Icon = i.severity === "critical" ? CircleX : i.severity === "warn" ? TriangleAlert : Info;
            return (
              <li key={idx} className={`issue ${i.severity}`}>
                <Icon className="i-icon" size={22} aria-hidden="true" />
                <div>
                  <div className="sev">{t(`results.issues.${i.severity}`)}</div>
                  <div>{tCode(t, i.code, i.params)}</div>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="card" style={{ marginTop: 36 }}>
        <button className="collapse-btn" aria-expanded={matrixOpen} aria-controls="matrix" onClick={() => setMatrixOpen((o) => !o)}>
          <span>
            <span style={{ fontSize: 20, fontWeight: 700, display: "block" }}>{t("results.matrix.title")}</span>
            <span className="hint">{t("results.matrix.subtitle")}</span>
          </span>
          <span className="chev"><ChevronDown aria-hidden="true" /></span>
        </button>
        {matrixOpen && (
          <div id="matrix" className="table-wrap fade-in">
            <table className="matrix">
              <thead>
                <tr>
                  <th scope="col">{t("results.matrix.test")}</th>
                  <th scope="col">{t("results.matrix.status")}</th>
                  <th scope="col">{t("results.matrix.score")}</th>
                  <th scope="col">{t("results.matrix.metrics")}</th>
                </tr>
              </thead>
              <tbody>
                {r.tests.map((x) => (
                  <tr key={x.id}>
                    <th scope="row" style={{ textTransform: "none", letterSpacing: 0, fontSize: 14, color: "var(--text)" }}>{t(`tests.${x.id}.name`)}</th>
                    <td><span className={`pill ${x.status}`}>{t(`status.${x.status}`)}</span></td>
                    <td>{x.score ?? "—"}</td>
                    <td>
                      {Object.keys(x.metrics).length === 0 ? <span className="hint">{t("results.matrix.none")}</span> : (
                        <div className="metrics-cell">
                          {Object.entries(x.metrics).map(([k, v]) => (
                            <span key={k}>{metricLabel(t, k)}: <b>{String(v)}</b></span>
                          ))}
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="result-actions">
        <button className="btn primary" aria-haspopup="menu" aria-expanded={menuOpen} onClick={() => setMenuOpen((o) => !o)}>
          <Download size={18} aria-hidden="true" /> {t("results.export")}
        </button>
        <button className="btn" onClick={onAnother}><RotateCcw size={18} aria-hidden="true" /> {t("results.another")}</button>
        {menuOpen && (
          <div className="card menu fade-in" role="menu" onKeyDown={(e) => e.key === "Escape" && setMenuOpen(false)}>
            <button className="btn ghost" role="menuitem" onClick={() => save("json")} autoFocus><FileJson size={18} aria-hidden="true" /> {t("results.exportJson")}</button>
            <button className="btn ghost" role="menuitem" onClick={() => save("html")}><FileText size={18} aria-hidden="true" /> {t("results.exportHtml")}</button>
            <button className="btn ghost" role="menuitem" onClick={print}><Printer size={18} aria-hidden="true" /> {t("results.print")}</button>
          </div>
        )}
      </div>
      <p className="toast" role="status">{toast}</p>
    </section>
  );
}
