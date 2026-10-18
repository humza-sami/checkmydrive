import { useEffect, useState } from "react";
import { Check, Circle, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Ring } from "../components/Ring";
import { tCode } from "../lib/codes";
import { formatDuration } from "../lib/format";
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

export function Running({ mode, startedAt, progress, cancelling, onCancel }: Props) {
  const { t } = useTranslation();
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
  const list = mode === "wipe" ? TEST_IDS : TEST_IDS;

  return (
    <section className="fade-in" aria-labelledby="run-title">
      <div className="page-head">
        <h1 id="run-title">{t("running.title")}</h1>
        <p>{t("running.subtitle")}</p>
      </div>
      <div className="run-wrap">
        <div className="card ring-card">
          <Ring value={pct} className={`ring ${progress ? "" : "pulse"}`} label={`${t("running.overall")}: ${pct}%`}>
            <span className="pct">{pct}%</span>
            <span className="sub">{t("running.overall")}</span>
          </Ring>
          <div>
            <div className="phase-name" aria-live="polite">{phaseName}</div>
            <p className="hint" aria-live="polite">{message}</p>
          </div>
          <div className="times">
            <span>{t("running.elapsed")}: <b>{formatDuration(elapsed)}</b></span>
            <span>{eta != null ? t("running.remaining", { time: formatDuration(eta) }) : t("running.estimating")}</span>
          </div>
          {!confirming ? (
            <button className="btn" onClick={() => setConfirming(true)} disabled={cancelling}>
              <X size={18} aria-hidden="true" /> {cancelling ? t("running.cancelling") : t("running.cancel")}
            </button>
          ) : (
            <div role="alertdialog" aria-label={t("running.cancel")} style={{ display: "grid", gap: 10 }}>
              <p className="hint">{t("running.cancelConfirm")}</p>
              <div style={{ display: "flex", gap: 10, justifyContent: "center" }}>
                <button className="btn" onClick={() => setConfirming(false)} autoFocus>{t("running.keepGoing")}</button>
                <button className="btn danger" onClick={() => { setConfirming(false); onCancel(); }}>{t("running.stop")}</button>
              </div>
            </div>
          )}
        </div>

        <div className="card">
          <h2 className="sr-only">{t("running.checklist")}</h2>
          <ul className="checklist" aria-label={t("running.checklist")}>
            {list.map((id) => {
              const s = statuses[id];
              const skipNote = id === "capacity_check" && mode === "safe";
              return (
                <li key={id} className={s}>
                  <span className="st-icon" aria-hidden="true">
                    {s === "done" ? <Check size={20} /> : s === "running" ? <LoaderCircle size={20} className="spin" /> : <Circle size={18} />}
                  </span>
                  <div className="grow">
                    <div className="t-name">{t(`tests.${id}.name`)}</div>
                    <div className="t-desc">{skipNote ? t("results.skippedNote") : t(`tests.${id}.desc`)}</div>
                  </div>
                  <span className="state-label">{t(`running.status.${s}`)}</span>
                </li>
              );
            })}
          </ul>
        </div>
      </div>
    </section>
  );
}
