# i18n keys emitted by the engine

Every human-readable string is a code. `{param}` placeholders are interpolated by the GUI (i18next).
Params are JSON numbers or strings. Severity is in brackets for issues. `en-US` sentences below are the source text.

**Plurals (GUI-side, no engine change):** the GUI passes `count` to i18next so sentences agree grammatically
(`_one`/`_other`/... suffixes per CLDR category in each locale file). Codes with a `count` param use it directly;
`issue.age.*` use `years`, and `issue.selftest.timeout` / `progress.selftest_short.running` (seconds variant) use
`seconds` as the count (table `COUNT_PARAM` in `app/src/lib/codes.ts`). The original param stays available to the text.

## Errors (`{"type":"error","code":...,"params"?}`; exit code in parentheses)

| code | params | en-US |
|---|---|---|
| `error.bad_args` (2) | - | The test could not start because of an internal request error. |
| `error.needs_admin` (3) | - | Administrator permission is needed to test this drive. |
| `error.needs_full_disk_access` (6) | - | macOS needs your permission before CheckMyDrive can read this drive. In System Settings, turn on Full Disk Access for “checkmydrive-engine”, then try again. |
| `error.drive_not_found` (4) | - | That drive could not be found. It may have been unplugged. |
| `error.system_disk_refused` (5) | - | This is the disk your computer runs from, so it cannot be erased. Choose another drive. |
| `error.confirm_required` (5) | - | Please confirm the drive's serial number before a full test that erases data. |
| `error.confirm_mismatch` (5) | - | The serial number you entered does not match this drive. Nothing was changed. |
| `error.cannot_open_device` (10) | `detail` (raw OS text, may be shown in a "details" box) | The drive could not be opened for testing. Another program may be using it. |
| `error.unmount_failed` (10) | `detail` | The drive's volumes could not be safely disconnected before the full test. Close any programs using it and try again. |
| `error.internal` (10) | - | Something unexpected went wrong inside the test engine. |

## Progress (`message_code` of `{"type":"progress"}`)

| code | params | en-US |
|---|---|---|
| `progress.prepare.starting` | - | Getting ready... |
| `progress.prepare.reading_smart` | - | Reading the drive's built-in health report... |
| `progress.prepare.unmounting` | - | Safely disconnecting the drive's volumes... |
| `progress.prepare.ready` | - | Ready to test. |
| `progress.drive_info.identifying` | - | Identifying the drive... |
| `progress.smart_health.analyzing` | - | Checking the drive's health report... |
| `progress.selftest_short.starting` | - | Starting the drive's built-in quick self-test... |
| `progress.selftest_short.running` | `percent` (0-100) or `seconds` (elapsed) | The drive is running its own quick self-test ({percent}% done). |
| `progress.selftest_short.done` | - | The drive's self-test finished. |
| `progress.surface_read.sampling` | `done`, `total` | Reading sample areas across the whole drive ({done} of {total}). |
| `progress.surface_read.sequential` | `mbps` | Measuring read speed ({mbps} MB/s). |
| `progress.surface_read.done` | - | Surface scan finished. |
| `progress.write_verify.writing` | `mb` | Writing test data ({mb} MB written). |
| `progress.write_verify.verifying` | `mb` | Reading the data back to check it ({mb} MB checked). |
| `progress.write_verify.cleaning` | - | Removing the temporary test file... |
| `progress.write_verify.done` | - | Write test finished. |
| `progress.capacity_check.writing` | `done`, `total` | Marking spots across the full claimed size ({done} of {total}). |
| `progress.capacity_check.verifying` | `done`, `total` | Checking that every spot holds its own data ({done} of {total}). |
| `progress.capacity_check.done` | - | Capacity check finished. |
| `progress.stability.checking` | - | Checking the drive stayed stable during the tests... |
| `progress.finish.scoring` | - | Working out your score... |
| `progress.finish.done` | - | Done. |

## Issues (`issue.code`, with severity)

### Age / identity
| code | sev | params | en-US |
|---|---|---|---|
| `issue.age.high_hours` | warn | `hours`, `years` | This drive has been powered on for about {years} years ({hours} hours), which is a lot of use. |
| `issue.age.moderate_hours` | info | `hours`, `years` | This drive has been powered on for about {years} years ({hours} hours). |
| `issue.drive_info.capacity_unknown` | warn | - | The drive did not report how big it is. |
| `issue.drive_info.model_unknown` | info | - | The drive did not report its model name. |
| `issue.drive_info.system_disk` | info | - | This is the disk your computer runs from. Only the safe, non-destructive test was run. |

### SMART health
| code | sev | params | en-US |
|---|---|---|---|
| `issue.smart.unavailable` | warn | - | The health-report tool (smartmontools) is not available, so the drive's internal health data could not be read. |
| `issue.smart.needs_admin` | warn | - | The drive's health report could not be read without administrator permission. |
| `issue.smart.timeout` | warn | - | The drive did not answer the health-report request in time. |
| `issue.smart.not_supported` | warn | - | This drive does not provide a health report, so its internal condition is unknown. |
| `issue.smart.usb_bridge_blocked` | info/warn | - | The USB adapter hides the drive's health report, so its internal condition is unknown. (info for USB sticks/SD cards, warn for external drives) |
| `issue.smart.overall_failed` | critical | - | The drive's own health check says it is failing. |
| `issue.smart.reallocated_sectors` | warn (critical if >100) | `count` | The drive has already replaced {count} damaged areas with spares. |
| `issue.smart.pending_sectors` | critical | `count` | {count} areas of the drive are unreadable and waiting to be replaced. |
| `issue.smart.uncorrectable_sectors` | critical | `count` | {count} areas of the drive could not be read or corrected. |
| `issue.smart.crc_errors` | warn | `count` | {count} data-transfer errors were recorded, often caused by a bad cable or connector. |
| `issue.smart.error_log_entries` | warn | `count` | The drive's error log contains {count} entries. |
| `issue.smart.ssd_worn` | warn | `percent` | This SSD has used up about {percent}% of its rated lifetime. |
| `issue.smart.ssd_worn_out` | critical | `percent` | This SSD has used up about {percent}% of its rated lifetime and is close to the end of its life. |
| `issue.smart.nvme_critical_warning` | critical | `value` | The drive is reporting a critical warning (code {value}). |
| `issue.smart.nvme_media_errors` | critical | `count` | The drive's memory has logged {count} data-integrity errors. |
| `issue.smart.spare_low` | warn/critical | `percent` | Only {percent}% of the drive's spare memory is left. |
| `issue.smart.temperature_high` | warn | `celsius` | The drive is running hot ({celsius} C). |

### Self-test
| code | sev | params | en-US |
|---|---|---|---|
| `issue.selftest.smart_unavailable` | info | - | The drive's quick self-test was skipped because its health interface is not available. |
| `issue.selftest.not_supported` | info | - | This drive does not support the quick self-test. |
| `issue.selftest.could_not_start` | info | - | The drive's quick self-test could not be started. |
| `issue.selftest.failed` | critical | - | The drive failed its own quick self-test. |
| `issue.selftest.aborted` | warn | - | The drive's quick self-test was interrupted before it finished. |
| `issue.selftest.timeout` | info | `seconds` | The drive's quick self-test did not finish within {seconds} seconds and was stopped. |

### Surface read
| code | sev | params | en-US |
|---|---|---|---|
| `issue.surface_read.cannot_open` | critical | `count` | The drive could not be opened for reading. |
| `issue.surface_read.read_errors` | critical | `count` | {count} parts of the drive could not be read. |
| `issue.surface_read.slow_areas` | warn/critical | `count`, `percent` | {count} areas ({percent}%) responded very slowly, which can mean the surface is wearing out. |
| `issue.surface_read.slow_drive` | info/warn | `mbps` | This drive is slow ({mbps} MB/s). That is fine for storing files, but too slow to run programs or an operating system comfortably. |

### Write test
| code | sev | params | en-US |
|---|---|---|---|
| `issue.write_verify.no_writable_volume` | info | - | The write test was skipped because the drive has no folder we can safely write to. |
| `issue.write_verify.insufficient_space` | info | - | The write test was skipped because the drive has too little free space. |
| `issue.write_verify.write_error` | critical | - | The drive reported an error while writing test data. |
| `issue.write_verify.data_mismatch` | critical | `count` | {count} blocks of test data came back different from what was written. |
| `issue.write_verify.slow_write` | warn | `mbps` | The drive writes very slowly ({mbps} MB/s). |
| `issue.write_verify.cleanup_failed` | warn | `path` | The temporary test file could not be deleted. You can remove {path} yourself. |

### Capacity check (full test only)
| code | sev | params | en-US |
|---|---|---|---|
| `issue.capacity_check.needs_wipe_mode` | info | - | The fake-capacity check needs the full test, which erases the drive. It was skipped. |
| `issue.capacity_check.fake_capacity` | critical | `claimed_bytes`, `real_bytes` | This drive claims to hold {claimed_bytes} bytes but only about {real_bytes} bytes are real. It is a fake-capacity drive. (GUI should format bytes as GB.) |
| `issue.capacity_check.data_corruption` | critical | `count` | {count} test blocks came back damaged. |
| `issue.capacity_check.io_error` | critical | `count` | The drive returned {count} errors during the capacity check. |

### Stability
| code | sev | params | en-US |
|---|---|---|---|
| `issue.stability.device_disconnected` | critical | `count` | The drive disappeared {count} time(s) during the tests. |
| `issue.stability.io_errors` | warn | `count` | The drive returned {count} read or write errors during the tests. |
| `issue.stability.timeouts` | warn | `count` | The drive timed out {count} time(s) during the tests. |
| `issue.stability.crc_errors_increased` | warn | `count` | Data-transfer errors increased by {count} during the tests. Check the cable. |
| `issue.stability.new_bad_sectors` | critical | `count` | {count} new damaged areas appeared while we were testing. |

## Suitability (`reason_code`; all carry `score`)

| code | en-US |
|---|---|
| `suitability.os_install.ok` | Good for installing an operating system on. |
| `suitability.os_install.bad_sectors` | Not recommended for an operating system: it has damaged areas. |
| `suitability.os_install.slow_speed` | Not recommended for an operating system: the drive reads too slowly. |
| `suitability.os_install.slow_latency` | Not recommended for an operating system: some reads are too slow. |
| `suitability.os_install.score_low` | Not recommended for an operating system: the overall score ({score}) is too low. |
| `suitability.long_term_storage.ok` | Good for storing files long term. |
| `suitability.long_term_storage.critical_issues` | Not safe for long-term storage: serious problems were found. |
| `suitability.long_term_storage.score_low` | Risky for long-term storage: the overall score ({score}) is too low. Keep backups. |
| `suitability.daily_use.ok` | Fine for everyday use. |
| `suitability.daily_use.caution` | Usable for everyday files, but keep backups. |
| `suitability.daily_use.no` | Not recommended for everyday use. |
