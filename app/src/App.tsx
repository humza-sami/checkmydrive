import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Header } from "./components/Header";
import { DrivePicker } from "./screens/DrivePicker";
import { ModeChoice } from "./screens/ModeChoice";
import { Results } from "./screens/Results";
import { Running } from "./screens/Running";
import { Settings } from "./screens/Settings";
import { EngineError, isMock, listDrives } from "./lib/engine";
import { applyTheme, readPref, type ThemePref } from "./lib/settings";
import { useTest } from "./lib/useTest";
import type { DriveInfo } from "./lib/types";
import { TriangleAlert } from "lucide-react";

export default function App() {
  const { t } = useTranslation();
  const { state, start, cancel, reset } = useTest();
  const [drives, setDrives] = useState<DriveInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [selected, setSelected] = useState<DriveInfo | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [mock, setMock] = useState(false);
  const [theme, setTheme] = useState<ThemePref>(() => (readPref("theme") as ThemePref) || "system");

  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => { isMock().then(setMock); }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setListError(null);
    try {
      setDrives(await listDrives());
    } catch (e) {
      setListError(e instanceof EngineError ? e.code : "error.engine_failed");
      setDrives([]);
    } finally {
      setLoading(false);
      isMock().then(setMock);
    }
  }, []);
  useEffect(() => { refresh(); }, [refresh]);

  const another = () => { reset(); setSelected(null); refresh(); };

  let body: React.ReactNode;
  if (showSettings) {
    body = <Settings theme={theme} onTheme={setTheme} onClose={() => setShowSettings(false)} />;
  } else if (state.status === "running") {
    body = (
      <Running drive={state.drive} mode={state.mode} startedAt={state.startedAt} progress={state.progress}
        cancelling={state.cancelling} onCancel={cancel} />
    );
  } else if (state.status === "done") {
    body = <Results report={state.report} onAnother={another} />;
  } else if (state.status === "error" || state.status === "cancelled") {
    body = (
      <section className="card fade-in" role="alert" style={{ textAlign: "center", padding: 40 }}>
        <TriangleAlert size={40} color="var(--caution)" aria-hidden="true" />
        <h1 style={{ fontSize: 24, margin: "12px 0" }}>{state.status === "error" ? t("error.title") : t("running.cancel")}</h1>
        {state.status === "error" && <p>{t(state.code, { defaultValue: t("error.unknown", { code: state.code }) })}</p>}
        <div style={{ display: "flex", gap: 12, justifyContent: "center", marginTop: 24, flexWrap: "wrap" }}>
          {state.status === "error" && (
            <button className="btn primary" onClick={() => { const d = state.drive; reset(); setSelected(d); }}>{t("error.retry")}</button>
          )}
          <button className="btn" onClick={another}>{t("error.choose")}</button>
        </div>
      </section>
    );
  } else if (selected) {
    body = <ModeChoice drive={selected} onBack={() => setSelected(null)} onStart={(m, s) => start(selected, m, s)} />;
  } else {
    body = <DrivePicker drives={drives} loading={loading} error={listError} onRefresh={refresh} onSelect={setSelected} />;
  }

  return (
    <div className="app">
      <a className="skip" href="#main">{t("app.skip")}</a>
      <Header onSettings={() => setShowSettings((s) => !s)} />
      {mock && <div className="banner" role="note">{t("nav.mockBanner")}</div>}
      <main id="main" className="main" tabIndex={-1}>{body}</main>
    </div>
  );
}
