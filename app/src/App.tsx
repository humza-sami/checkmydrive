import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AboutDialog } from "./components/AboutDialog";
import { Dialog } from "./components/Dialog";
import { DriveTabs } from "./components/DriveTabs";
import { MenuBar, type MenuDef } from "./components/MenuBar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { XpIcon, XpIconSprite } from "./components/XpIcon";
import { LANGUAGES, setLanguage } from "./i18n";
import { EngineError, getEngineInfo, isMock, listDrives, type EngineInfo } from "./lib/engine";
import { openExternal, REPO_URL } from "./lib/links";
import { FullDiskAccessDialog } from "./components/FullDiskAccessDialog";
import { buildHtmlReport, reportFileName, saveTextFile } from "./lib/report";
import { useTest } from "./lib/useTest";
import { closeWindow } from "./lib/windowControls";
import type { DriveInfo, Mode, TestReport } from "./lib/types";
import { ModeChoice } from "./screens/ModeChoice";
import { Overview } from "./screens/Overview";
import { Results } from "./screens/Results";
import { Running } from "./screens/Running";

export default function App() {
  const { t, i18n } = useTranslation();
  const { state, start, cancel, reset } = useTest();
  const [drives, setDrives] = useState<DriveInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [reports, setReports] = useState<Record<string, TestReport>>({});
  const [dialog, setDialog] = useState<null | "mode" | "about">(null);
  const [mock, setMock] = useState(false);
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const say = useCallback((msg: string) => {
    setToast(msg);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 4000);
  }, []);

  useEffect(() => { isMock().then(setMock); getEngineInfo().then(setInfo); }, []);

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

  // keep a valid selection as the drive list changes
  useEffect(() => {
    if (!drives) return;
    setSelectedId((cur) => (cur && drives.some((d) => d.id === cur) ? cur : drives[0]?.id ?? null));
  }, [drives]);

  // remember each finished report under its drive so the tab keeps its result
  useEffect(() => {
    if (state.status === "done") setReports((m) => ({ ...m, [state.drive.id]: state.report }));
  }, [state]);

  const running = state.status === "running";
  const selected = drives?.find((d) => d.id === selectedId) ?? null;
  const report = selected ? reports[selected.id] : undefined;

  const doStart = (mode: Mode, serial?: string) => {
    if (!selected) return;
    setDialog(null);
    start(selected, mode, serial);
  };
  const openStart = () => { if (selected && !running) setDialog("mode"); };

  const saveReport = async (kind: "json" | "html") => {
    if (!report) return;
    try {
      const dir = document.documentElement.dir === "rtl" ? "rtl" : "ltr";
      const ok = kind === "json"
        ? await saveTextFile(reportFileName(report, "json"), JSON.stringify(report, null, 2), "application/json")
        : await saveTextFile(reportFileName(report, "html"), buildHtmlReport(report, t, i18n.language, dir), "text/html");
      if (ok) say(t("results.saved"));
    } catch {
      say(t("results.saveFailed"));
    }
  };
  const printReport = () => { if (report) setTimeout(() => window.print(), 100); };

  const menus: MenuDef[] = [
    {
      id: "file", label: t("menu.file"),
      items: [
        { id: "save-json", label: `${t("results.exportJson")}…`, disabled: !report, onSelect: () => saveReport("json") },
        { id: "save-html", label: `${t("results.exportHtml")}…`, disabled: !report, onSelect: () => saveReport("html") },
        { id: "print", label: `${t("results.print")}…`, disabled: !report, onSelect: printReport },
        { id: "sep1", separator: true },
        { id: "exit", label: t("menu.exit"), onSelect: closeWindow },
      ],
    },
    {
      id: "test", label: t("menu.test"),
      items: [
        { id: "start", label: t("menu.startTest"), disabled: !selected || running, onSelect: openStart },
        { id: "cancel", label: t("running.cancel"), disabled: !running || state.status !== "running" || state.cancelling, onSelect: () => { void cancel(); } },
        { id: "sep2", separator: true },
        { id: "refresh", label: t("menu.refresh"), disabled: loading || running, onSelect: refresh },
      ],
    },
    {
      id: "view", label: t("menu.view"),
      items: [
        {
          id: "language", label: t("settings.language"),
          submenu: LANGUAGES.map((l) => ({ id: `lang:${l.code}`, label: l.name, checked: i18n.language === l.code, onSelect: () => { void setLanguage(l.code); } })),
        },
      ],
    },
    {
      id: "help", label: t("menu.help"),
      items: [
        { id: "github", label: t("menu.github"), onSelect: () => openExternal(REPO_URL) },
        { id: "sep3", separator: true },
        { id: "about", label: t("menu.about"), onSelect: () => setDialog("about") },
      ],
    },
  ];

  const another = () => { reset(); refresh(); };

  let body: React.ReactNode;
  if (state.status === "running") {
    body = <Running drive={state.drive} mode={state.mode} startedAt={state.startedAt} progress={state.progress} cancelling={state.cancelling} onCancel={cancel} />;
  } else if (loading && !drives) {
    body = <p className="pad-msg">{t("picker.refreshing")}</p>;
  } else if (!selected) {
    body = (
      <div className="empty">
        <XpIcon name="hdd" size={32} />
        <p>{t("picker.empty")}</p>
        {listError && <p role="alert"><XpIcon name="warn" size={16} className="inl" /> {t("picker.loadFailed")} {t(listError, { defaultValue: t("error.unknown", { code: listError }) })}</p>}
      </div>
    );
  } else {
    body = (
      <>
        {listError && <p role="alert" className="pad-msg"><XpIcon name="warn" size={16} className="inl" /> {t("picker.loadFailed")} {t(listError, { defaultValue: t("error.unknown", { code: listError }) })}</p>}
        {report ? <Results report={report} onAnother={another} /> : <Overview drive={selected} onStart={openStart} />}
      </>
    );
  }

  const message = toast ?? (running ? t("statusbar.testing") : loading ? t("picker.refreshing") : t("statusbar.ready"));
  const failed = state.status === "error" || state.status === "cancelled" ? state : null;

  return (
    <div className="app">
      <XpIconSprite />
      <a className="skip" href="#main">{t("app.skip")}</a>
      <TitleBar />
      <MenuBar menus={menus} label={t("app.name")} />
      {mock && <div className="infobar" role="note"><XpIcon name="info" size={16} /> {t("nav.mockBanner")}</div>}
      {drives && drives.length > 0 && (
        <DriveTabs drives={drives} selectedId={selectedId} reports={reports} disabled={running} onSelect={setSelectedId} />
      )}
      <main id="main" className="main" tabIndex={-1} role="tabpanel" aria-busy={loading}>{body}</main>
      <StatusBar message={message} info={info} mock={mock} driveCount={drives?.length ?? 0} />

      {dialog === "mode" && selected && <ModeChoice drive={selected} onStart={doStart} onCancel={() => setDialog(null)} />}
      {dialog === "about" && <AboutDialog onClose={() => setDialog(null)} />}
      {failed?.status === "error" && failed.code === "error.needs_full_disk_access" && <FullDiskAccessDialog onClose={another} />}
      {failed && !(failed.status === "error" && failed.code === "error.needs_full_disk_access") && (
        <Dialog title={failed.status === "error" ? t("error.title") : t("running.cancel")} role="alertdialog" width={400} onClose={another}>
          <div className="msgbox">
            <XpIcon name={failed.status === "error" ? "error" : "warn"} size={32} />
            <p>{failed.status === "error" ? t(failed.code, { defaultValue: t("error.unknown", { code: failed.code }) }) : t("running.cancelled")}</p>
          </div>
          <div className="dlg-buttons">
            {failed.status === "error" && (
              <button type="button" className="default" data-autofocus onClick={() => { const d = failed.drive; reset(); setSelectedId(d.id); setDialog("mode"); }}>{t("error.retry")}</button>
            )}
            <button type="button" onClick={another} data-autofocus={failed.status === "cancelled" ? true : undefined}>{t("error.choose")}</button>
          </div>
        </Dialog>
      )}
    </div>
  );
}
