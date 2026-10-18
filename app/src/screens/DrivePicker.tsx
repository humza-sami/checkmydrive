import heroArt from "../assets/hero.png";
import emptyArt from "../assets/empty-no-drives.png";
import { RefreshCw, ShieldAlert, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DriveIcon } from "../components/DriveIcon";
import { formatBytes } from "../lib/format";
import type { DriveInfo } from "../lib/types";

interface Props {
  drives: DriveInfo[] | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onSelect: (d: DriveInfo) => void;
}

export function DrivePicker({ drives, loading, error, onRefresh, onSelect }: Props) {
  const { t, i18n } = useTranslation();
  return (
    <section className="fade-in" aria-labelledby="picker-title">
      <div className="page-head">
        <div>
          <h1 id="picker-title">{t("picker.title")}</h1>
          <p>{t("picker.subtitle")}</p>
        </div>
        <img className="head-art" src={heroArt} alt="" />
      </div>
      <div className="toolbar">
        <button className="btn" onClick={onRefresh} disabled={loading}>
          <RefreshCw size={18} className={loading ? "spin" : ""} aria-hidden="true" />
          {loading ? t("picker.refreshing") : t("picker.refresh")}
        </button>
      </div>
      {error && (
        <div className="card" role="alert" style={{ marginBottom: 16 }}>
          <TriangleAlert aria-hidden="true" /> {t("picker.loadFailed")} {t(error, { defaultValue: t("error.unknown", { code: error }) })}
        </div>
      )}
      {drives && drives.length === 0 && !loading && <div className="card empty"><img src={emptyArt} alt="" />{t("picker.empty")}</div>}
      <ul className="grid" style={{ listStyle: "none", padding: 0, margin: 0 }} aria-busy={loading}>
        {(drives ?? []).map((d) => (
          <li key={d.id}>
            <button className="card drive-card" onClick={() => onSelect(d)} aria-label={`${t("picker.select")}: ${d.model ?? t("picker.unknownModel")}, ${formatBytes(d.capacity_bytes, i18n.language)}`}>
              <div className="drive-head">
                <div className="drive-icon"><DriveIcon kind={d.kind} /></div>
                {d.is_system_disk && (
                  <span className="chip warn" title={t("picker.systemDiskHint")}>
                    <ShieldAlert size={13} aria-hidden="true" style={{ verticalAlign: "-2px" }} /> {t("picker.systemDisk")}
                  </span>
                )}
              </div>
              <div>
                <div className="drive-model">{d.model ?? t("picker.unknownModel")}</div>
                <div className="hint" style={{ marginTop: 2 }}>{t(`picker.kind.${d.kind}`)}</div>
              </div>
              <div className="drive-cap">{formatBytes(d.capacity_bytes, i18n.language)}</div>
              <div className="chips">
                {d.bus && <span className="chip">{d.bus.toUpperCase()}</span>}
                {d.kind === "hdd" && d.rotation_rpm ? <span className="chip">{d.rotation_rpm} RPM</span> : null}
                {d.removable && <span className="chip">{t("picker.removable")}</span>}
                <span className="chip accent">{d.id}</span>
              </div>
            </button>
          </li>
        ))}
      </ul>
      <p className="hint" style={{ marginTop: 22 }}>{t("picker.noAdminNote")}</p>
    </section>
  );
}
