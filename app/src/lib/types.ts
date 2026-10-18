// Mirrors docs/CONTRACT.md (snake_case JSON emitted by checkmydrive-engine).
export type DriveKind = "hdd" | "ssd" | "nvme" | "usb" | "sd" | "unknown";

export interface DriveInfo {
  id: string;
  path: string;
  model: string | null;
  serial: string | null;
  firmware: string | null;
  vendor: string | null;
  capacity_bytes: number;
  kind: DriveKind;
  bus: string | null;
  rotation_rpm: number | null;
  removable: boolean;
  is_system_disk: boolean;
  mount_points: string[];
  smart_supported: boolean | null;
  power_on_hours: number | null;
  manufacture_date: string | null;
}

export type TestId =
  | "drive_info"
  | "smart_health"
  | "selftest_short"
  | "surface_read"
  | "write_verify"
  | "capacity_check"
  | "stability";

export const TEST_IDS: TestId[] = [
  "drive_info",
  "smart_health",
  "selftest_short",
  "surface_read",
  "write_verify",
  "capacity_check",
  "stability",
];

export type Params = Record<string, string | number>;
export type Status = "pass" | "warn" | "fail" | "skipped";
export type Severity = "info" | "warn" | "critical";
export type Mode = "safe" | "wipe";
export type Verdict = "buy" | "caution" | "avoid";

export interface ProgressEvent {
  type: "progress";
  phase: TestId | "prepare" | "finish";
  percent: number;
  phase_percent: number;
  message_code: string;
  params?: Params;
}

export interface Issue {
  severity: Severity;
  code: string;
  params?: Params;
}

export interface TestResult {
  id: TestId;
  status: Status;
  score: number | null;
  duration_ms: number;
  metrics: Record<string, number | string | boolean | null>;
  issues: Issue[];
}

export interface Suitability {
  ok: boolean;
  level: "good" | "caution" | "no";
  reason_code: string;
  params?: Params;
}

export interface TestReport {
  schema_version: 1;
  engine_version: string;
  drive: DriveInfo;
  mode: Mode;
  started_at: string;
  duration_ms: number;
  tests: TestResult[];
  score: number;
  verdict: Verdict;
  issues: Issue[];
  age: {
    power_on_hours: number | null;
    power_on_years: number | null;
    manufacture_date: string | null;
    power_cycles: number | null;
  };
  suitability: {
    os_install: Suitability;
    long_term_storage: Suitability;
    daily_use: Suitability;
    safe_fill_bytes: number;
    safe_fill_percent: number;
  };
}

export type EngineEvent =
  | ProgressEvent
  | { type: "report"; report: TestReport }
  | { type: "error"; code: string; params?: Params }
  | { type: "cancelled" };
