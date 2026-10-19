import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "../components/Dialog";
import { DriveIcon, XpIcon } from "../components/XpIcon";
import { formatBytes } from "../lib/format";
import type { DriveInfo, Mode } from "../lib/types";

interface Props {
  drive: DriveInfo;
  onStart: (mode: Mode, confirmSerial?: string) => void;
  onCancel: () => void;
}

/** XP dialog: pick quick (safe) or full (wipe) test; wipe asks for the serial number (or the device id when the drive has none). */
export function ModeChoice({ drive, onStart, onCancel }: Props) {
  const { t, i18n } = useTranslation();
  const [mode, setMode] = useState<Mode>("safe");
  const [typed, setTyped] = useState("");
  const uid = useId();
  const model = drive.model ?? t("picker.unknownModel");
  const serial = drive.serial?.trim() ?? "";
  // What the user must type: the serial number, or the device id (e.g. "disk6") when the drive reports no serial.
  const token = serial || drive.id;
  const byId = serial === "";
  const blocked = drive.is_system_disk ? "systemBlocked" : null;
  const matches = typed.trim() === token;
  const canStart = mode === "safe" || (!blocked && matches);

  const submit = () => { if (canStart) onStart(mode, mode === "wipe" ? token : undefined); };

  return (
    <Dialog title={t("menu.startTest")} onClose={onCancel} width={470}>
      <form onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <div className="dlg-head">
          <DriveIcon kind={drive.kind} size={32} />
          <div>
            <div>{t("mode.checking")}: <b>{model}</b> · {formatBytes(drive.capacity_bytes, i18n.language)}</div>
          </div>
        </div>

        <fieldset className="radio-group">
          <legend>{t("mode.title")}</legend>
          <div className="radio-row">
            <input type="radio" id={`${uid}-safe`} name={`${uid}-mode`} checked={mode === "safe"} onChange={() => setMode("safe")} data-autofocus />
            <label htmlFor={`${uid}-safe`}>{t("mode.safe.title")} <span className="badge">{t("mode.safe.badge")}</span></label>
          </div>
          <p className="radio-desc">{t("mode.safe.desc")} {t("mode.safe.limit")}<br />{t("mode.duration")}</p>

          <div className="radio-row">
            <input type="radio" id={`${uid}-wipe`} name={`${uid}-mode`} checked={mode === "wipe"} onChange={() => setMode("wipe")} disabled={!!blocked} />
            <label htmlFor={`${uid}-wipe`}>{t("mode.wipe.title")}</label>
          </div>
          <p className="radio-desc">{t("mode.wipe.desc")}</p>
          {blocked && <p className="radio-desc note" role="status">{t(`mode.wipe.${blocked}`)}</p>}
        </fieldset>

        {mode === "wipe" && !blocked && (
          <div className="warn-box" role="group" aria-label={t("mode.wipe.dangerTitle")}>
            <XpIcon name="warn" size={32} />
            <div className="grow">
              <div className="dlg-strong">{t("mode.wipe.dangerTitle")}</div>
              <p>{t("mode.wipe.dangerBody", { model })}</p>
              <label htmlFor={`${uid}-serial`} className="serial-label">{byId ? t("mode.wipe.confirmLabelId", { id: token }) : t("mode.wipe.confirmLabel", { serial })}</label>
              <input
                id={`${uid}-serial`} type="text" className="serial-input" value={typed} onChange={(e) => setTyped(e.target.value)}
                placeholder={t(byId ? "mode.wipe.confirmPlaceholderId" : "mode.wipe.confirmPlaceholder")} autoComplete="off" autoCorrect="off" autoCapitalize="off" spellCheck={false}
                aria-invalid={typed !== "" && !matches} aria-describedby={`${uid}-hint`} dir="ltr"
              />
              <p id={`${uid}-hint`} className="serial-hint" aria-live="polite">{typed && !matches ? t("mode.wipe.mismatch") : " "}</p>
            </div>
          </div>
        )}

        <div className="dlg-buttons">
          <button type="submit" className="default" disabled={!canStart}>{mode === "wipe" ? t("mode.wipe.start") : t("menu.startTest")}</button>
          <button type="button" onClick={onCancel}>{t("common.cancel")}</button>
        </div>
      </form>
    </Dialog>
  );
}
