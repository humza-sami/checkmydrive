import React from "react";
import ReactDOM from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import App from "./App";
import { initI18n } from "./i18n";
import { applyTheme, readPref, type ThemePref } from "./lib/settings";
import "./styles.css";

applyTheme((readPref("theme") as ThemePref) || "system");
initI18n().then((i18n) => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>
    </React.StrictMode>,
  );
});
