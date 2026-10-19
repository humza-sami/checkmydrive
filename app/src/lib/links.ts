import { inTauri } from "./engine";

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
