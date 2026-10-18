import { useCallback, useEffect, useRef, useState } from "react";
import type { DriveInfo, Mode, ProgressEvent, TestReport } from "./types";
import { cancelTest, startTest } from "./engine";

export type RunState =
  | { status: "idle" }
  | { status: "running"; drive: DriveInfo; mode: Mode; startedAt: number; progress: ProgressEvent | null; cancelling: boolean }
  | { status: "done"; drive: DriveInfo; report: TestReport }
  | { status: "error"; drive: DriveInfo; mode: Mode; code: string }
  | { status: "cancelled"; drive: DriveInfo };

/**
 * The engine runs the drive's self-test at the same time as the surface scan, so their progress
 * frames interleave. While the surface scan is active we show only its frames (the overall
 * percent is monotonic in both), so the screen does not flicker between two phases.
 */
export function isBackgroundFrame(surfaceActive: boolean, e: ProgressEvent): boolean {
  return surfaceActive && e.phase === "selftest_short";
}

export function nextSurfaceActive(prev: boolean, e: ProgressEvent): boolean {
  if (e.phase !== "surface_read") return prev;
  return e.message_code !== "progress.surface_read.done" && e.phase_percent < 100;
}

/** Owns the lifecycle of one test run: start, stream progress, cancel, finish. */
export function useTest() {
  const [state, setState] = useState<RunState>({ status: "idle" });
  const unsub = useRef<(() => void) | null>(null);
  const surfaceActive = useRef(false);

  const cleanup = () => {
    unsub.current?.();
    unsub.current = null;
  };
  useEffect(() => cleanup, []);

  const start = useCallback(async (drive: DriveInfo, mode: Mode, confirmSerial?: string) => {
    cleanup();
    surfaceActive.current = false;
    setState({ status: "running", drive, mode, startedAt: Date.now(), progress: null, cancelling: false });
    try {
      unsub.current = await startTest(drive, mode, confirmSerial, (e) => {
        if (e.type === "progress") {
          if (isBackgroundFrame(surfaceActive.current, e)) return;
          surfaceActive.current = nextSurfaceActive(surfaceActive.current, e);
          setState((s) => (s.status === "running" ? { ...s, progress: e } : s));
        } else if (e.type === "report") {
          cleanup();
          setState({ status: "done", drive, report: e.report });
        } else if (e.type === "error") {
          cleanup();
          setState({ status: "error", drive, mode, code: e.code });
        } else if (e.type === "cancelled") {
          cleanup();
          setState({ status: "cancelled", drive });
        }
      });
    } catch (err) {
      const code = (err as { code?: string }).code ?? "error.engine_failed";
      setState({ status: "error", drive, mode, code });
    }
  }, []);

  const cancel = useCallback(async () => {
    setState((s) => (s.status === "running" ? { ...s, cancelling: true } : s));
    await cancelTest();
  }, []);

  const reset = useCallback(() => {
    cleanup();
    setState({ status: "idle" });
  }, []);

  return { state, start, cancel, reset };
}
