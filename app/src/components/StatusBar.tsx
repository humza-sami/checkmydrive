import { useTranslation } from "react-i18next";
import type { EngineInfo } from "../lib/engine";

interface Props {
  message: string;
  info: EngineInfo | null;
  mock: boolean;
  driveCount: number;
}

export function StatusBar({ message, info, mock, driveCount }: Props) {
  const { t } = useTranslation();
  const engine = info ? t("results.engine", { version: info.version }) : mock ? t("results.engine", { version: "demo" }) : "—";
  const smart = info?.smartctl ? t("statusbar.smartctl", { version: info.smartctl }) : null;
  return (
    <div className="statusbar" role="status" aria-live="polite">
      <div className="status-field grow">{message}</div>
      <div className="status-field">{[engine, smart].filter(Boolean).join(" · ")}</div>
      <div className="status-field">{t("statusbar.drives", { n: driveCount })}</div>
      <div className="status-grip" aria-hidden="true" />
    </div>
  );
}
