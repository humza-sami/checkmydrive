import { ExternalLink, Monitor, Moon, Sun, ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { LANGUAGES, setLanguage } from "../i18n";
import { inTauri } from "../lib/engine";
import { applyTheme, writePref, type ThemePref } from "../lib/settings";

export const REPO_URL = "https://github.com/humza-sami/checkmydrive";
export const APP_VERSION = "0.1.0";

export async function openExternal(url: string) {
  if (inTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

interface Props {
  theme: ThemePref;
  onTheme: (t: ThemePref) => void;
  onClose: () => void;
}

export function Settings({ theme, onTheme, onClose }: Props) {
  const { t, i18n } = useTranslation();
  const themes: { v: ThemePref; icon: typeof Sun; label: string }[] = [
    { v: "system", icon: Monitor, label: t("settings.themeSystem") },
    { v: "light", icon: Sun, label: t("settings.themeLight") },
    { v: "dark", icon: Moon, label: t("settings.themeDark") },
  ];
  const pick = (v: ThemePref) => {
    onTheme(v);
    writePref("theme", v);
    applyTheme(v);
  };
  return (
    <section className="fade-in" aria-labelledby="set-title">
      <button className="btn ghost" onClick={onClose} style={{ marginBottom: 8 }}>
        <ArrowLeft size={18} className="flip" aria-hidden="true" /> {t("nav.back")}
      </button>
      <div className="page-head"><h1 id="set-title">{t("settings.title")}</h1></div>
      <div className="card">
        <div className="set-row">
          <div>
            <label htmlFor="lang" style={{ fontWeight: 700 }}>{t("settings.language")}</label>
            <p className="hint" style={{ maxWidth: "46ch" }}>{t("settings.languageHint")}</p>
          </div>
          <select id="lang" className="field" value={i18n.language} onChange={(e) => setLanguage(e.target.value)}>
            {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.name}</option>)}
          </select>
        </div>
        <div className="set-row">
          <div style={{ fontWeight: 700 }} id="theme-label">{t("settings.theme")}</div>
          <div className="seg" role="radiogroup" aria-labelledby="theme-label">
            {themes.map(({ v, icon: Icon, label }) => (
              <button key={v} role="radio" aria-checked={theme === v} onClick={() => pick(v)}>
                <Icon size={16} aria-hidden="true" /> {label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <h2 className="section-title">{t("settings.about")}</h2>
      <div className="card">
        <div className="set-row"><span>{t("settings.version")}</span><strong>{APP_VERSION}</strong></div>
        <div className="set-row"><span>{t("settings.license")}</span><strong>{t("settings.licenseValue")}</strong></div>
        <div className="set-row">
          <a className="link" href={REPO_URL} onClick={(e) => { e.preventDefault(); openExternal(REPO_URL); }}>
            {t("settings.github")} <ExternalLink size={16} aria-hidden="true" />
          </a>
          <span className="hint">{REPO_URL.replace("https://", "")}</span>
        </div>
        <p className="hint" style={{ marginTop: 14 }}>{t("settings.disclaimer")}</p>
      </div>
    </section>
  );
}
