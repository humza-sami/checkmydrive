import { useTranslation } from "react-i18next";
import { formatBytes, scoreTone } from "../lib/format";
import type { DriveInfo, TestReport } from "../lib/types";
import { DriveIcon } from "./XpIcon";

interface Props {
  drives: DriveInfo[];
  selectedId: string | null;
  reports: Record<string, TestReport>;
  disabled: boolean;
  onSelect: (id: string) => void;
}

/** Row of drive tabs, one per drive, each with a health-colored indicator (grey until tested). */
export function DriveTabs({ drives, selectedId, reports, disabled, onSelect }: Props) {
  const { t, i18n } = useTranslation();
  const move = (e: React.KeyboardEvent, i: number) => {
    const rtl = document.documentElement.dir === "rtl";
    const step = e.key === "ArrowRight" ? (rtl ? -1 : 1) : e.key === "ArrowLeft" ? (rtl ? 1 : -1) : 0;
    if (!step || !drives.length) return;
    e.preventDefault();
    const n = drives[(i + step + drives.length) % drives.length];
    onSelect(n.id);
    requestAnimationFrame(() => document.getElementById(`tab-${n.id}`)?.focus());
  };
  return (
    <div className="drive-tabs" role="tablist" aria-label={t("picker.title")}>
      {drives.map((d, i) => {
        const r = reports[d.id];
        const tone = r ? scoreTone(r.score) : "none";
        const sel = d.id === selectedId;
        const model = d.model ?? t("picker.unknownModel");
        const cap = formatBytes(d.capacity_bytes, i18n.language);
        return (
          <button
            key={d.id}
            id={`tab-${d.id}`}
            type="button"
            role="tab"
            aria-selected={sel}
            aria-controls="main"
            tabIndex={sel ? 0 : -1}
            disabled={disabled && !sel}
            className={`drive-tab ${sel ? "selected" : ""}`}
            onClick={() => onSelect(d.id)}
            onKeyDown={(e) => move(e, i)}
            aria-label={`${t("picker.select")}: ${model}, ${cap}`}
          >
            <span className={`dot ${tone}`} title={r ? t(`health.${tone === "good" ? "good" : tone === "caution" ? "caution" : "bad"}`) : t("health.untested")} />
            <DriveIcon kind={d.kind} size={16} />
            <span className="tab-text">
              <span className="tab-model">{model}</span>
              <span className="tab-sub"><bdi dir="ltr">{cap} · {d.id}</bdi>{d.is_system_disk ? ` · ${t("picker.systemDisk")}` : ""}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
