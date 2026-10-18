import { useId, useState } from "react";
import { ArrowLeft, ChevronDown, ShieldCheck, Skull } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DriveIcon } from "../components/DriveIcon";
import { formatBytes } from "../lib/format";
import type { DriveInfo, Mode } from "../lib/types";

interface Props {
  drive: DriveInfo;
  onStart: (mode: Mode, confirmSerial?: string) => void;
  onBack: () => void;
}

export function ModeChoice({ drive, onStart, onBack }: Props) {
  const { t, i18n } = useTranslation();
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const inputId = useId();
  const model = drive.model ?? t("picker.unknownModel");
  const serial = drive.serial?.trim() ?? "";
  const blocked = drive.is_system_disk ? "systemBlocked" : !serial ? "noSerial" : null;
  const matches = serial !== "" && typed.trim() === serial;

  return (
    <section className="fade-in" aria-labelledby="mode-title">
      <button className="btn ghost" onClick={onBack} style={{ marginBottom: 8 }}>
        <ArrowLeft size={18} className="flip" aria-hidden="true" /> {t("mode.back")}
      </button>
      <div className="page-head">
        <h1 id="mode-title">{t("mode.title")}</h1>
        <p style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <DriveIcon kind={drive.kind} size={20} />
          <span>{t("mode.checking")}: <strong style={{ color: "var(--text)" }}>{model}</strong> · {formatBytes(drive.capacity_bytes, i18n.language)}</span>
        </p>
      </div>

      <div className="stack">
        <div className="card mode-card selected">
          <h2>
            <ShieldCheck aria-hidden="true" color="var(--good)" /> {t("mode.safe.title")}
            <span className="chip accent">{t("mode.safe.badge")}</span>
          </h2>
          <p>{t("mode.safe.desc")}</p>
          <p className="hint">{t("mode.safe.limit")}</p>
          <div className="actions">
            <button className="btn primary" onClick={() => onStart("safe")}>{t("mode.start")}</button>
            <span className="hint">{t("mode.duration")}</span>
          </div>
        </div>

        <div>
          <button className="btn" aria-expanded={open} aria-controls="wipe-zone" onClick={() => setOpen((o) => !o)}>
            <Skull size={18} aria-hidden="true" color="var(--bad)" />
            {open ? t("mode.wipe.collapse") : t("mode.wipe.expand")}
            <ChevronDown size={18} aria-hidden="true" style={{ transform: open ? "rotate(180deg)" : "none", transition: "transform .2s" }} />
          </button>
        </div>

        {open && (
          <div id="wipe-zone" className="danger-zone fade-in" role="group" aria-labelledby="wipe-title">
            <h2 id="wipe-title"><Skull aria-hidden="true" /> {t("mode.wipe.dangerTitle")}</h2>
            <p><strong>{t("mode.wipe.title")}</strong></p>
            <p>{t("mode.wipe.desc")}</p>
            {blocked ? (
              <p className="hint bad" role="status" style={{ marginTop: 14 }}>{t(`mode.wipe.${blocked}`)}</p>
            ) : (
              <>
                <p><strong>{t("mode.wipe.dangerBody", { model })}</strong></p>
                <label htmlFor={inputId}>{t("mode.wipe.confirmLabel", { serial })}</label>
                <input
                  id={inputId} className="field" value={typed} onChange={(e) => setTyped(e.target.value)}
                  placeholder={t("mode.wipe.confirmPlaceholder")} autoComplete="off" autoCorrect="off" autoCapitalize="off" spellCheck={false}
                  aria-invalid={typed !== "" && !matches} aria-describedby={`${inputId}-hint`}
                />
                <p id={`${inputId}-hint`} className={`hint ${typed && !matches ? "bad" : ""}`} aria-live="polite">
                  {typed && !matches ? t("mode.wipe.mismatch") : " "}
                </p>
                <div style={{ marginTop: 14 }}>
                  <button className="btn danger" disabled={!matches} onClick={() => onStart("wipe", serial)}>
                    {t("mode.wipe.start")}
                  </button>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
