# CheckMyDrive — architecture contract

Two parts, built independently, joined only by this contract.

```
checkmydrive/
├── engine/            Rust crate `checkmydrive-engine` (lib + CLI bin). All disk logic. No UI text.
├── app/               Tauri v2 desktop app (React + TypeScript + Vite). Calls the engine CLI as a sidecar.
│   ├── src/           Frontend
│   └── src-tauri/     Tauri shell (standalone crate, NOT a cargo workspace with engine)
├── docs/
└── .github/workflows/
```

## Engine CLI

All output is JSON. Progress is NDJSON (one JSON object per line). Never prints human prose to stdout; logs go to stderr.

- `checkmydrive-engine list` → stdout: `DriveInfo[]` (one JSON array). Must work WITHOUT admin rights (basic info via diskutil / lsblk / PowerShell `Get-Disk`+`Get-PhysicalDisk`); SMART-derived fields may be null when unprivileged.
- `checkmydrive-engine test --drive <id> --mode safe|wipe [--confirm-serial <serial>] [--out <file>]`
  → NDJSON lines of `{"type":"progress",...}` then exactly one `{"type":"report","report":TestReport}`, or `{"type":"error","code":...}`. If `--out` is given, the same lines are also appended to that file (used on macOS, where the elevated process can't pipe to the GUI).
  - `wipe` requires `--confirm-serial`; ALWAYS refuses the system/boot disk; unmounts the drive first. The confirmation token ("what the user must type") is the drive's **serial number** when it has one (compared case- and whitespace-insensitively; either the OS-reported or the SMART serial is accepted). When the drive reports **no serial at all**, the token is its **device id as shown in the UI** (`disk6`, `sdb`, `PhysicalDrive2`; `/dev/` / `/dev/rdisk6` spellings are accepted too). When a serial exists the id is NOT accepted. The app shows the exact token in the wipe dialog, so a non-technical user only copies what they see. Missing token -> `error.confirm_required`, wrong token -> `error.confirm_mismatch` (both exit 5, nothing is touched).
- `checkmydrive-engine version` → `{"version":"x.y.z","smartctl":"7.x"|null}`

Exit codes: 0 ok, 2 bad args, 3 needs admin, 4 drive not found, 5 refused (system disk / bad confirm), 6 needs Full Disk Access (macOS), 10 internal.

Raw-device `PermissionDenied` (EPERM/EACCES) mapping: on macOS when the engine already runs as root (`geteuid() == 0`) the block is TCC privacy protection, so the engine emits `{"type":"error","code":"error.needs_full_disk_access"}` (exit 6): the responsible app (CheckMyDrive.app) needs *System Settings -> Privacy & Security -> Full Disk Access*. In every other case it stays `error.needs_admin` (exit 3). The app answers `needs_full_disk_access` with a dialog and an "Open Settings" button (`x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`).

macOS identity fallback: `list`/`test` fill missing `serial`, `vendor`, `firmware` of USB drives from the IORegistry (`ioreg -r -c IOUSBHostDevice -l -w0`, the nearest `IOUSBHostDevice` above the disk's `BSD Name`; firmware = `bcdDevice`). Generic vendors ("USB", "Generic") and all-zero serials are ignored. Values from diskutil/system_profiler/SMART always win.

smartctl lookup order: next to the engine executable → `resources/` dir beside it → PATH → `/usr/sbin`, `/usr/local/sbin`, `/opt/homebrew/sbin`, `C:\Program Files\smartmontools\bin`. Missing smartctl ⇒ SMART tests `skipped` with issue `smart.unavailable`, other tests still run.

## Privilege / elevation (GUI side)

- macOS: `osascript -e 'do shell script "<engine> test ... --out <tmp>" with administrator privileges'`; GUI tails `<tmp>`.
- Linux: `pkexec <engine> test ...`; read stdout.
- Windows: app manifest `requireAdministrator`; spawn engine directly.

## Types (JSON, snake_case)

```ts
type DriveKind = "hdd" | "ssd" | "nvme" | "usb" | "sd" | "unknown";

interface DriveInfo {
  id: string;                 // stable id to pass back to `test` (e.g. "disk4", "/dev/sdb", "PhysicalDrive2")
  path: string;               // raw device path
  model: string | null;
  serial: string | null;
  firmware: string | null;
  vendor: string | null;
  capacity_bytes: number;
  kind: DriveKind;
  bus: string | null;         // "usb" | "sata" | "nvme" | "sas" | ...
  rotation_rpm: number | null;
  removable: boolean;
  is_system_disk: boolean;
  mount_points: string[];
  smart_supported: boolean | null;
  power_on_hours: number | null;
  manufacture_date: string | null; // ISO "YYYY-MM" or "YYYY-Www" only when the drive reports it
}

interface ProgressEvent {
  type: "progress";
  phase: TestId | "prepare" | "finish";
  percent: number;            // 0-100 overall
  phase_percent: number;      // 0-100 within phase
  message_code: string;       // i18n key, e.g. "progress.surface_read.sampling"
  params?: Record<string, string | number>;
}

type TestId = "drive_info" | "smart_health" | "selftest_short" | "surface_read" | "write_verify" | "capacity_check" | "stability";
type Status = "pass" | "warn" | "fail" | "skipped";

interface Issue {
  severity: "info" | "warn" | "critical";
  code: string;               // i18n key, e.g. "issue.smart.reallocated_sectors"
  params?: Record<string, string | number>;  // values interpolated into the translated text
}

interface TestResult {
  id: TestId;
  status: Status;
  score: number | null;       // 0-100, null when skipped
  duration_ms: number;
  metrics: Record<string, number | string | boolean | null>; // e.g. read_mbps_avg, latency_ms_p99, slow_samples, read_errors
  issues: Issue[];
}

interface Suitability { ok: boolean; level: "good" | "caution" | "no"; reason_code: string; params?: Record<string, string|number>; }

interface TestReport {
  schema_version: 1;
  engine_version: string;
  drive: DriveInfo;
  mode: "safe" | "wipe";
  started_at: string;         // ISO 8601
  duration_ms: number;
  tests: TestResult[];        // always all 7 ids, in the order above
  score: number;              // 0-100 overall
  verdict: "buy" | "caution" | "avoid";
  issues: Issue[];            // all issues, sorted critical → info
  age: { power_on_hours: number | null; power_on_years: number | null; manufacture_date: string | null; power_cycles: number | null };
  suitability: {
    os_install: Suitability;
    long_term_storage: Suitability;
    daily_use: Suitability;
    safe_fill_bytes: number;  // how much data we'd recommend storing
    safe_fill_percent: number;
  };
}
```

## Test plan (target total ≈ 3–4 minutes)

| id | what | budget |
|---|---|---|
| drive_info | identify drive, kind, capacity, age | <5 s |
| smart_health | smartctl -a -j (with `-d sat`/`-d auto` fallbacks for USB bridges): overall verdict, reallocated / pending / uncorrectable sectors, CRC errors, SSD wear / spare / media errors (NVMe), temperature, error log | <10 s |
| selftest_short | `smartctl -t short`, poll until done; skipped if unsupported | ≤ 2.5 min (runs concurrently with surface_read where possible) |
| surface_read | read-only raw-device sampling: ≥512 evenly spaced + random 1 MiB reads across the whole capacity, plus a 15 s sequential read; latency per sample, slow (>300 ms HDD / >50 ms SSD) and failed samples, throughput | ~60 s |
| write_verify | safe mode: temp file `.checkmydrive-<rand>.tmp` on a mounted writable volume of that drive, size min(1 GiB, 5% free), PRNG pattern, fsync, cache-bypassing read-back (F_NOCACHE / O_DIRECT / FILE_FLAG_NO_BUFFERING), verify, delete. wipe mode: same on raw device regions. Skipped (issue `issue.write_verify.no_writable_volume`) if no writable mount. | ~30 s |
| capacity_check | wipe mode only: write unique signatures (offset+nonce) to ≥256 sectors spread across claimed capacity, read all back; mismatch/aliasing ⇒ fake capacity (critical). Safe mode: `skipped` with `issue.capacity_check.needs_wipe_mode` (info). | ~20 s |
| stability | observed during all tests: device disappearing/reappearing, I/O errors, timeouts, CRC error delta before/after | passive |

## Scoring (engine, unit-tested)

Start at 100; each issue deducts by severity and magnitude; hard caps: SMART overall FAILED → ≤10; fake capacity → ≤5; device disconnected during test → ≤25; any pending/uncorrectable sectors → ≤45; reallocated > 0 → ≤70 (≤40 if > 100); SSD wear ≥ 90 % used → ≤40; read errors → ≤30.
Verdict: score ≥ 80 `buy`, 50–79 `caution`, < 50 `avoid`.
Suitability: os_install good iff score ≥ 80 and no pending/reallocated sectors and p99 latency OK; long_term_storage good iff score ≥ 75 and no critical issues; safe_fill_percent = 90 (≥80), 60 (65–79), 30 (50–64), 0 (<50).

## i18n

The engine emits only codes + params. All text lives in `app/src/locales/<lang>.json` (i18next). `en-US` is default and source of truth. RTL languages (ar, ur, fa, he) must render RTL.
