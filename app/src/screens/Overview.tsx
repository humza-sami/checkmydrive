import { useTranslation } from "react-i18next";
import { HealthTile, Properties } from "../components/DriveOverview";
import { XpIcon } from "../components/XpIcon";
import type { DriveInfo } from "../lib/types";

/** Shown for a drive that has not been tested yet in this session. */
export function Overview({ drive, onStart }: { drive: DriveInfo; onStart: () => void }) {
  const { t } = useTranslation();
  return (
    <section aria-label={drive.model ?? t("picker.unknownModel")} className="results">
      <div className="top-row">
        <fieldset className="health-box">
          <legend>{t("health.title")}</legend>
          <div className="health-inner">
            <HealthTile tone="none" score={null} />
            <div className="health-text">
              <p className="summary">{t("idle.hint")}</p>
              <p className="meta">{t("picker.noAdminNote")}</p>
              <button type="button" className="default start-btn" onClick={onStart}>{t("menu.startTest")}</button>
            </div>
          </div>
        </fieldset>
        <fieldset className="props-box">
          <legend>{t("props.title")}</legend>
          <Properties drive={drive} />
          {drive.is_system_disk && <p className="foot"><XpIcon name="info" size={12} className="inl" /> {t("picker.systemDiskHint")}</p>}
        </fieldset>
      </div>
    </section>
  );
}
