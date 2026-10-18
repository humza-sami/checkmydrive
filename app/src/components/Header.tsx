import { Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";

export function Header({ onSettings }: { onSettings: () => void }) {
  const { t } = useTranslation();
  return (
    <header className="topbar">
      <div className="brand">
        <img src={logo} alt="" width={30} height={30} />
        <span>{t("app.name")}</span>
      </div>
      <button className="btn ghost icon-btn" onClick={onSettings} aria-label={t("nav.settings")} title={t("nav.settings")}>
        <Settings size={22} aria-hidden="true" />
      </button>
    </header>
  );
}
