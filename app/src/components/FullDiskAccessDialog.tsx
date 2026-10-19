import { useTranslation } from "react-i18next";
import { openExternal } from "../lib/links";
import { Dialog } from "./Dialog";
import { XpIcon } from "./XpIcon";

/** macOS deep link to Privacy & Security -> Full Disk Access. */
export const FULL_DISK_ACCESS_URL = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

interface Props {
  onClose: () => void;
}

/** Shown when the engine reports `error.needs_full_disk_access` (macOS privacy protection blocks raw disk access). */
export function FullDiskAccessDialog({ onClose }: Props) {
  const { t } = useTranslation();
  return (
    <Dialog title={t("fda.title")} role="alertdialog" width={440} onClose={onClose}>
      <div className="msgbox">
        <XpIcon name="warn" size={32} />
        <div>
          <p>{t("fda.intro")}</p>
          <ol>
            {[1, 2, 3, 4].map((n) => <li key={n}>{t(`fda.step${n}`)}</li>)}
          </ol>
        </div>
      </div>
      <div className="dlg-buttons">
        <button type="button" className="default" data-autofocus onClick={() => { void openExternal(FULL_DISK_ACCESS_URL); }}>{t("fda.openSettings")}</button>
        <button type="button" onClick={onClose}>{t("common.close")}</button>
      </div>
    </Dialog>
  );
}
