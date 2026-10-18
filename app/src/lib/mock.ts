import type {
  DriveInfo, EngineEvent, Issue, Mode, ProgressEvent, TestId, TestReport, TestResult,
} from "./types";
import { TEST_IDS } from "./types";
import { hoursToYears } from "./format";

const GB = 1_000_000_000;

export const MOCK_DRIVES: DriveInfo[] = [
  {
    id: "disk0", path: "/dev/disk0", model: "APPLE SSD AP0512Q", serial: "0ba0245c0a7a7b02", firmware: "1161.100.", vendor: "Apple",
    capacity_bytes: 500 * GB, kind: "nvme", bus: "nvme", rotation_rpm: null, removable: false, is_system_disk: true,
    mount_points: ["/"], smart_supported: true, power_on_hours: 3120, manufacture_date: null,
  },
  {
    id: "disk4", path: "/dev/disk4", model: "Samsung Portable SSD T7", serial: "S5SXNJ0W412345X", firmware: "PJT2", vendor: "Samsung",
    capacity_bytes: 1000 * GB, kind: "ssd", bus: "usb", rotation_rpm: null, removable: true, is_system_disk: false,
    mount_points: ["/Volumes/T7"], smart_supported: true, power_on_hours: 1460, manufacture_date: "2023-08",
  },
  {
    id: "disk5", path: "/dev/disk5", model: "WDC WD20NMVW-11AV3S3", serial: "WD-WXH1A8512QLK", firmware: "01.01A01", vendor: "Western Digital",
    capacity_bytes: 2000 * GB, kind: "hdd", bus: "usb", rotation_rpm: 5400, removable: true, is_system_disk: false,
    mount_points: ["/Volumes/Backup"], smart_supported: true, power_on_hours: 41230, manufacture_date: "2017-11",
  },
  {
    id: "disk6", path: "/dev/disk6", model: "Generic Flash Disk", serial: "AA00000000000489", firmware: null, vendor: null,
    capacity_bytes: 1024 * GB, kind: "usb", bus: "usb", rotation_rpm: null, removable: true, is_system_disk: false,
    mount_points: ["/Volumes/USB1TB"], smart_supported: false, power_on_hours: null, manufacture_date: null,
  },
];

function t(id: TestId, status: TestResult["status"], score: number | null, ms: number,
  metrics: TestResult["metrics"] = {}, issues: Issue[] = []): TestResult {
  return { id, status, score, duration_ms: ms, metrics, issues };
}

function build(drive: DriveInfo, mode: Mode, tests: TestResult[], score: number,
  verdict: TestReport["verdict"], extra: Partial<TestReport>): TestReport {
  const issues = tests.flatMap((x) => x.issues);
  const order = { critical: 0, warn: 1, info: 2 } as const;
  issues.sort((a, b) => order[a.severity] - order[b.severity]);
  return {
    schema_version: 1,
    engine_version: "0.1.0-mock",
    drive, mode,
    started_at: new Date().toISOString(),
    duration_ms: 214_000,
    tests, score, verdict, issues,
    age: {
      power_on_hours: drive.power_on_hours,
      power_on_years: hoursToYears(drive.power_on_hours),
      manufacture_date: drive.manufacture_date,
      power_cycles: drive.power_on_hours ? Math.round(drive.power_on_hours / 11) : null,
    },
    suitability: {
      os_install: { ok: true, level: "good", reason_code: "suitability.os_install.ok" },
      long_term_storage: { ok: true, level: "good", reason_code: "suitability.long_term_storage.ok" },
      daily_use: { ok: true, level: "good", reason_code: "suitability.daily_use.ok" },
      safe_fill_bytes: 0, safe_fill_percent: 0,
    },
    ...extra,
  };
}

export function mockReport(drive: DriveInfo, mode: Mode): TestReport {
  const wipe = mode === "wipe";
  const capSkipped = t("capacity_check", "skipped", null, 0, {}, [
    { severity: "info", code: "issue.capacity_check.needs_wipe_mode" },
  ]);
  if (drive.id === "disk5") {
    const tests = [
      t("drive_info", "pass", 100, 800, { rpm: 5400, interface: "USB 3.0" }),
      t("smart_health", "fail", 12, 4200, { reallocated_sectors: 312, pending_sectors: 24, uncorrectable_sectors: 8, temperature_c: 44 }, [
        { severity: "critical", code: "issue.smart.pending_sectors", params: { count: 24 } },
        { severity: "critical", code: "issue.smart.reallocated_sectors", params: { count: 312 } },
        { severity: "warn", code: "issue.age.high_hours", params: { hours: 41200, years: 4.7 } },
      ]),
      t("selftest_short", "fail", 0, 118_000, { result: "read failure" }, [{ severity: "critical", code: "issue.selftest.failed" }]),
      t("surface_read", "fail", 34, 61_000, { read_mbps_avg: 58.4, latency_ms_p99: 640, slow_samples: 37, read_errors: 5 }, [
        { severity: "critical", code: "issue.surface_read.read_errors", params: { count: 5 } },
        { severity: "warn", code: "issue.surface_read.slow_areas", params: { count: 37, percent: 5.8 } },
      ]),
      t("write_verify", "warn", 70, 33_000, { written_mb: 1024, mismatches: 0, write_mbps: 41.2 }),
      wipe ? t("capacity_check", "pass", 100, 18_000, { sectors_checked: 256, mismatches: 0 }) : capSkipped,
      t("stability", "fail", 20, 0, { disconnects: 3, io_errors: 11 }, [
        { severity: "critical", code: "issue.stability.device_disconnected", params: { count: 3 } },
      ]),
    ];
    return build(drive, mode, tests, 8, "avoid", {
      suitability: {
        os_install: { ok: false, level: "no", reason_code: "suitability.os_install.bad_sectors" },
        long_term_storage: { ok: false, level: "no", reason_code: "suitability.long_term_storage.critical_issues" },
        daily_use: { ok: false, level: "no", reason_code: "suitability.daily_use.no" },
        safe_fill_bytes: 0, safe_fill_percent: 0,
      },
    });
  }
  if (drive.id === "disk6") {
    const fake = wipe;
    const tests = [
      t("drive_info", "warn", 60, 600, { claimed_gb: 1024 }, [{ severity: "info", code: "issue.drive_info.model_unknown" }]),
      t("smart_health", "skipped", null, 300, {}, [{ severity: "info", code: "issue.smart.unavailable" }]),
      t("selftest_short", "skipped", null, 0, {}, [{ severity: "info", code: "issue.selftest.not_supported" }]),
      t("surface_read", "pass", 85, 60_000, { read_mbps_avg: 31.2, latency_ms_p99: 22, slow_samples: 1, read_errors: 0 }),
      t("write_verify", fake ? "fail" : "pass", fake ? 10 : 90, 30_000, { written_mb: 512, mismatches: fake ? 188 : 0, write_mbps: 9.8 },
        fake ? [{ severity: "critical", code: "issue.write_verify.data_mismatch", params: { count: 188 } }] : []),
      fake
        ? t("capacity_check", "fail", 0, 21_000, { sectors_checked: 256, mismatches: 240, real_capacity_gb: 32 }, [
            { severity: "critical", code: "issue.capacity_check.fake_capacity", params: { claimed_bytes: 1_024_000_000_000, real_bytes: 32_000_000_000 } },
          ])
        : capSkipped,
      t("stability", "pass", 100, 0, { disconnects: 0, io_errors: 0 }),
    ];
    return build(drive, mode, tests, fake ? 4 : 62, fake ? "avoid" : "caution", {
      suitability: fake
        ? {
            os_install: { ok: false, level: "no", reason_code: "suitability.os_install.score_low", params: { score: 4 } },
            long_term_storage: { ok: false, level: "no", reason_code: "suitability.long_term_storage.critical_issues" },
            daily_use: { ok: false, level: "no", reason_code: "suitability.daily_use.no" },
            safe_fill_bytes: 0, safe_fill_percent: 0,
          }
        : {
            os_install: { ok: false, level: "caution", reason_code: "suitability.os_install.score_low", params: { score: 62 } },
            long_term_storage: { ok: false, level: "caution", reason_code: "suitability.long_term_storage.score_low", params: { score: 62 } },
            daily_use: { ok: true, level: "caution", reason_code: "suitability.daily_use.caution" },
            safe_fill_bytes: Math.round(drive.capacity_bytes * 0.3), safe_fill_percent: 30,
          },
    });
  }
  const t7 = drive.id === "disk4";
  const tests = [
    t("drive_info", "pass", 100, 700, { interface: t7 ? "USB 3.2 Gen 2" : "NVMe PCIe" }),
    t("smart_health", "pass", t7 ? 98 : 94, 3100, { reallocated_sectors: 0, percent_used: t7 ? 2 : 6, temperature_c: t7 ? 36 : 41, media_errors: 0 }),
    t("selftest_short", "pass", 100, 96_000, { result: "completed without error" }),
    t("surface_read", "pass", t7 ? 96 : 92, 60_000, { read_mbps_avg: t7 ? 912 : 2310, latency_ms_p99: 4.1, slow_samples: 0, read_errors: 0 }),
    t("write_verify", "pass", 100, 24_000, { written_mb: 1024, mismatches: 0, write_mbps: t7 ? 640 : 1800 }),
    wipe ? t("capacity_check", "pass", 100, 17_000, { sectors_checked: 256, mismatches: 0 }) : capSkipped,
    t("stability", "pass", 100, 0, { disconnects: 0, io_errors: 0 }),
  ];
  return build(drive, mode, tests, t7 ? 96 : 87, "buy", {
    suitability: {
      os_install: { ok: true, level: "good", reason_code: "suitability.os_install.ok" },
      long_term_storage: { ok: true, level: "good", reason_code: "suitability.long_term_storage.ok" },
      daily_use: { ok: true, level: "good", reason_code: "suitability.daily_use.ok" },
      safe_fill_bytes: Math.round(drive.capacity_bytes * 0.9), safe_fill_percent: 90,
    },
  });
}

/** Simulated ~20 s test. Returns a cancel function. */
export function runMockTest(drive: DriveInfo, mode: Mode, emit: (e: EngineEvent) => void, totalMs = 20_000): () => void {
  const phases: { phase: ProgressEvent["phase"]; code: string; weight: number }[] = [
    { phase: "prepare", code: "progress.prepare.starting", weight: 4 },
    { phase: "drive_info", code: "progress.drive_info.identifying", weight: 6 },
    { phase: "smart_health", code: "progress.smart_health.analyzing", weight: 10 },
    { phase: "selftest_short", code: "progress.selftest_short.running", weight: 22 },
    { phase: "surface_read", code: "progress.surface_read.sampling", weight: 26 },
    { phase: "write_verify", code: "progress.write_verify.writing", weight: 16 },
    { phase: "capacity_check", code: "progress.capacity_check.writing", weight: 10 },
    { phase: "stability", code: "progress.stability.checking", weight: 4 },
    { phase: "finish", code: "progress.finish.scoring", weight: 2 },
  ];
  const sum = phases.reduce((a, p) => a + p.weight, 0);
  const start = Date.now();
  const timer = setInterval(() => {
    const frac = Math.min(1, (Date.now() - start) / totalMs);
    const pct = frac * 100;
    let acc = 0;
    for (const p of phases) {
      const lo = (acc / sum) * 100;
      acc += p.weight;
      const hi = (acc / sum) * 100;
      if (pct <= hi || p === phases[phases.length - 1]) {
        emit({
          type: "progress", phase: p.phase, percent: Math.round(pct * 10) / 10,
          phase_percent: Math.min(100, Math.round(((pct - lo) / (hi - lo)) * 100)),
          message_code: p.code, params: { done: Math.round(((pct - lo) / (hi - lo)) * 512), total: 512, percent: Math.round(((pct - lo) / (hi - lo)) * 100), mb: Math.round(((pct - lo) / (hi - lo)) * 512) },
        });
        break;
      }
    }
    if (frac >= 1) {
      clearInterval(timer);
      emit({ type: "report", report: mockReport(drive, mode) });
    }
  }, 250);
  return () => {
    clearInterval(timer);
    emit({ type: "cancelled" });
  };
}

export const MOCK_TEST_IDS = TEST_IDS;
