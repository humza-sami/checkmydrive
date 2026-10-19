import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "../components/Dialog";
import { ProgressBar } from "../components/ProgressBar";
import { DriveIcon, XpIcon } from "../components/XpIcon";
import { tCode } from "../lib/codes";
import { formatBytes, formatDuration } from "../lib/format";
import { TEST_IDS, type DriveInfo, type Mode, type ProgressEvent, type TestId } from "../lib/types";

interface Props {
  drive: DriveInfo;
  mode: Mode;
  startedAt: number;
  progress: ProgressEvent | null;
  cancelling: boolean;
  onCancel: () => void;
}

type LiveStatus = "pending" | "running" | "done";

export function liveStatuses(progress: ProgressEvent | null): Record<TestId, LiveStatus> {
  const out = {} as Record<TestId, LiveStatus>;
  const cur = progress ? TEST_IDS.indexOf(progress.phase as TestId) : -1;
  const finishing = progress?.phase === "finish";
  TEST_IDS.forEach((id, i) => {
    if (finishing) out[id] = "done";
    else if (!progress || progress.phase === "prepare") out[id] = "pending";
    else if (i < cur) out[id] = "done";
    else if (progress.phase === "selftest_short" && id === "surface_read") out[id] = "done"; // runs alongside the scan
    else if (i === cur) out[id] = progress.phase_percent >= 100 ? "done" : "running";
    else out[id] = "pending";
  });
  return out;
}

const ICON = { done: "ok", running: "play", pending: "pending" } as const;

export function Running({ drive, mode, startedAt, progress, cancelling, onCancel }: Props) {
  const { t, i18n } = useTranslation();
  const [now, setNow] = useState(Date.now());
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, []);

  const elapsed = (now - startedAt) / 1000;
  const pct = Math.round(progress?.percent ?? 0);
  const eta = progress && progress.percent >= 5 ? (elapsed * (100 - progress.percent)) / progress.percent : null;
  const statuses = liveStatuses(progress);
  const phaseName = !progress
    ? t("running.waiting")
    : progress.phase === "prepare" || progress.phase === "finish"
      ? t(`running.phase.${progress.phase}`)
      : t(`tests.${progress.phase}.name`);
  const message = progress ? tCode(t, progress.message_code, progress.params, "progress.working") : t("running.adminPrompt");

  return (
    <section aria-labelledby="run-title" className="running">
      <fieldset>
        <legend id="run-title">{t("running.title")}</legend>
        <div className="run-head">
          <DriveIcon kind={drive.kind} size={32} />
          <div>
            <div className="dlg-strong">{drive.model ?? t("picker.unknownModel")} · {formatBytes(drive.capacity_bytes, i18n.language)}</div>
            <div>{t(`results.mode.${mode}`)} — {t("running.subtitle")}</div>
          </div>
        </div>
        <div className="phase-line" aria-live="polite"><b>{phaseName}</b></div>
        <div className="msg-line" aria-live="polite">{message}</div>
        <div className="progress-row">
          <ProgressBar value={pct} label={`${t("running.overall")}: ${pct}%`} />
          <span className="pct" aria-hidden="true"><bdi dir="ltr">{pct}%</bdi></span>
        </div>
        <div className="times">
          <span>{t("running.elapsed")}: <b><bdi dir="ltr">{formatDuration(elapsed)}</bdi></b></span>
          <span>{eta != null ? t("running.remaining", { time: formatDuration(eta) }) : t("running.estimating")}</span>
        </div>
      </fieldset>

      <fieldset>
        <legend>{t("running.checklist")}</legend>
        <ul className="checklist" aria-label={t("running.checklist")}>
          {TEST_IDS.map((id) => {
            const s = statuses[id];
            const skipNote = id === "capacity_check" && mode === "safe";
            return (
              <li key={id} className={s}>
                <XpIcon name={ICON[s]} size={16} />
                <span className="t-name">{t(`tests.${id}.name`)}</span>
                <span className="t-desc">{skipNote ? t("results.skippedNote") : t(`tests.${id}.desc`)}</span>
                <span className="state-label">{t(`running.status.${s}`)}</span>
              </li>
            );
          })}
        </ul>
      </fieldset>

      <div className="run-actions">
        <button type="button" onClick={() => setConfirming(true)} disabled={cancelling}>{cancelling ? t("running.cancelling") : t("running.cancel")}</button>
      </div>

      {confirming && (
        <Dialog title={t("running.cancel")} role="alertdialog" width={380} onClose={() => setConfirming(false)}>
          <div className="msgbox">
            <XpIcon name="question" size={32} />
            <p>{t("running.cancelConfirm")}</p>
          </div>
          <div className="dlg-buttons">
            <button type="button" className="default" onClick={() => setConfirming(false)} data-autofocus>{t("running.keepGoing")}</button>
            <button type="button" onClick={() => { setConfirming(false); onCancel(); }}>{t("running.stop")}</button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
