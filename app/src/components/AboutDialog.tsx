import { useTranslation } from "react-i18next";
import appIcon from "../../src-tauri/icons/128x128.png";
import { APP_VERSION, openExternal, REPO_URL } from "../lib/links";
import { Dialog } from "./Dialog";

export function AboutDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Dialog title={t("menu.about")} onClose={onClose} width={420}>
      <div className="about">
        <img src={appIcon} width={48} height={48} alt="" />
        <div className="about-text">
          <div className="about-name">{t("app.name")}</div>
          <div>{t("settings.version")} {APP_VERSION}</div>
          <div>{t("settings.license")}: {t("settings.licenseValue")}</div>
          <div>
            <a className="link" href={REPO_URL} onClick={(e) => { e.preventDefault(); openExternal(REPO_URL); }}>{t("settings.github")}</a>
            {" "}<span className="dim">{REPO_URL.replace("https://", "")}</span>
          </div>
        </div>
      </div>
      <hr className="etched" />
      <p className="about-note">{t("about.credits")}</p>
      <p className="about-note">{t("settings.disclaimer")}</p>
      <div className="dlg-buttons">
        <button type="button" className="default" onClick={onClose} data-autofocus>{t("common.ok")}</button>
      </div>
    </Dialog>
  );
}
