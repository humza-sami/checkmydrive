import { useTranslation } from "react-i18next";
import appIcon from "../../src-tauri/icons/32x32.png";
import { closeWindow, minimizeWindow, toggleMaximizeWindow } from "../lib/windowControls";

/** Luna caption buttons: glyphs are tiny SVGs so they stay crisp and need no images. */
const GLYPH = {
  min: <svg width="9" height="9" viewBox="0 0 9 9" aria-hidden="true"><rect x="0" y="6" width="7" height="3" fill="#fff" /></svg>,
  max: <svg width="9" height="9" viewBox="0 0 9 9" aria-hidden="true"><path d="M.5.5h8v8h-8z" fill="none" stroke="#fff" /><rect x="0" y="0" width="9" height="2.4" fill="#fff" /></svg>,
  close: <svg width="9" height="9" viewBox="0 0 9 9" aria-hidden="true"><path d="M1 1l7 7M8 1 1 8" stroke="#fff" strokeWidth="2" /></svg>,
};

export function TitleBar() {
  const { t } = useTranslation();
  return (
    <div className="titlebar" data-tauri-drag-region onDoubleClick={undefined}>
      <img className="titlebar-icon" src={appIcon} width={16} height={16} alt="" data-tauri-drag-region />
      <div className="titlebar-text" data-tauri-drag-region>{t("app.name")}</div>
      <div className="titlebar-controls">
        <button type="button" className="cap" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={minimizeWindow}>{GLYPH.min}</button>
        <button type="button" className="cap" aria-label={t("window.maximize")} title={t("window.maximize")} onClick={toggleMaximizeWindow}>{GLYPH.max}</button>
        <button type="button" className="cap close" aria-label={t("window.close")} title={t("window.close")} onClick={closeWindow}>{GLYPH.close}</button>
      </div>
    </div>
  );
}
