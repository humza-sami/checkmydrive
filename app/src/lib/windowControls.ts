import { inTauri } from "./engine";

/** Window chrome actions. Outside Tauri (browser / mock mode) they do nothing. */
async function withWindow(fn: (w: import("@tauri-apps/api/window").Window) => Promise<void>) {
  if (!inTauri()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await fn(getCurrentWindow());
  } catch {
    /* window API unavailable: ignore */
  }
}

export const minimizeWindow = () => withWindow((w) => w.minimize());
export const toggleMaximizeWindow = () => withWindow((w) => w.toggleMaximize());
export const closeWindow = () => withWindow((w) => w.close());
