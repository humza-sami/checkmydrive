import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DriveInfo, EngineEvent, Mode } from "./types";
import { MOCK_DRIVES, runMockTest } from "./mock";

export const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const forcedMock = import.meta.env.VITE_MOCK === "1" || import.meta.env.VITE_MOCK === "true";

let mockMode: boolean | null = null;

/** True when the UI runs against fake drives (VITE_MOCK=1, plain browser, or sidecar missing). */
export async function isMock(): Promise<boolean> {
  if (mockMode !== null) return mockMode;
  if (forcedMock || !inTauri()) return (mockMode = true);
  try {
    mockMode = !(await invoke<boolean>("engine_available"));
  } catch {
    mockMode = true;
  }
  return mockMode;
}

export class EngineError extends Error {
  constructor(public code: string, message?: string) {
    super(message ?? code);
  }
}

export async function listDrives(): Promise<DriveInfo[]> {
  if (await isMock()) {
    await new Promise((r) => setTimeout(r, 450));
    return MOCK_DRIVES;
  }
  try {
    return await invoke<DriveInfo[]>("list_drives");
  } catch (e) {
    const msg = String(e);
    throw new EngineError(msg.startsWith("engine_missing") ? "error.engine_missing" : "error.engine_failed", msg);
  }
}

let cancelMock: (() => void) | null = null;

/** Starts a test; `onEvent` receives progress / report / error / cancelled. Returns an unsubscribe fn. */
export async function startTest(
  drive: DriveInfo,
  mode: Mode,
  confirmSerial: string | undefined,
  onEvent: (e: EngineEvent) => void,
): Promise<() => void> {
  if (await isMock()) {
    cancelMock = runMockTest(drive, mode, onEvent);
    return () => { cancelMock = null; };
  }
  const unlisten = await listen<EngineEvent>("engine-event", (ev) => onEvent(ev.payload));
  try {
    await invoke("start_test", { driveId: drive.id, mode, confirmSerial: confirmSerial ?? null });
  } catch (e) {
    unlisten();
    throw new EngineError("error.engine_failed", String(e));
  }
  return unlisten;
}

export async function cancelTest(): Promise<void> {
  if (cancelMock) {
    const c = cancelMock;
    cancelMock = null;
    c();
    return;
  }
  if (inTauri()) await invoke("cancel_test");
}
