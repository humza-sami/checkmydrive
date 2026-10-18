# checkmydrive-engine

Rust crate (lib + CLI) that tests a used drive and emits JSON. Spec: `../docs/CONTRACT.md`.
Text for every emitted code: `I18N_KEYS.md`. The engine never prints prose to stdout.

```
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -- list                                   # unprivileged
cargo run -- test --drive disk4 --mode safe         # needs root/admin (raw device read)
cargo run -- test --drive disk4 --mode wipe --confirm-serial <serial>   # DESTRUCTIVE
cargo test && cargo clippy --all-targets
cargo check --target x86_64-pc-windows-gnu          # Windows compile check
```

## Layout
- `src/cli.rs` clap + exit codes; `src/engine.rs` orchestration, safety gates, report assembly.
- `src/platform/{macos,linux,windows}.rs` enumeration, raw paths, system-disk detection, unmount, cache-bypass I/O. `platform/mod.rs` has `RawDevice` (read-only unless opened with `write=true`, which only wipe mode does).
- `src/smart.rs` smartctl lookup/run/parse; USB-bridge fallbacks (`-d` none, sat, auto, sntjmicron, sntrealtek, sntasmedia, scsi).
- `src/tests/*.rs` the seven tests; `src/score.rs` pure scoring; `src/report.rs` wire types; `src/progress.rs` NDJSON emitter (+1 s heartbeat that re-sends the last frame); `src/cleanup.rs` temp-file guard.
- `tests/` integration tests + `tests/fixtures/*.json` (realistic `smartctl -a -j` output).

## Safety rules (do not weaken)
- Safe mode: raw device opened read-only; the only write is one `.checkmydrive-<rand>.tmp` created with `create_new`, registered in a global registry only after creation, deleted by RAII guard on every exit path and by the Ctrl-C/SIGTERM handler.
- Wipe mode: refuses `is_system_disk` (exit 5) and requires `--confirm-serial` equal to the drive serial (OS serial or SMART serial; if the drive reports no serial, its id). Unmounts first. Only 4 x <=64 MiB regions + 321 4-KiB blocks are overwritten (not a full wipe).
- Never run wipe against a real device while developing. `tests/engine_flow.rs` uses sparse files.

## Scoring metrics (keys read by `score::Facts`)
smart_health: `smart_passed`, `reallocated_sectors`, `pending_sectors`, `uncorrectable_sectors`, `wear_used_percent`; surface_read: `read_errors`, `p99_latency_ok`; write_verify: `verify_mismatches`, `write_errors`; capacity_check: `fake_capacity`; stability: `device_disconnected`.

## smartctl
Lookup: `$CHECKMYDRIVE_SMARTCTL`, next to the exe, `resources/`, `../Resources`, PATH, OS sbin dirs, (debug builds) `vendor/<os>/smartctl`. `vendor/` is gitignored; fetch the macOS one from the smartmontools GitHub release `.dmg` (`pkgutil --expand-full` the pkg, copy `usr/local/sbin/smartctl`).

## Known gaps
- Windows code is compile-checked only (never executed). Windows `Get-Disk` JSON parse and `\\.\PhysicalDriveN` NO_BUFFERING reads are untested on real hardware; `smartctl` target is `/dev/sdX` (letter = drive number).
- Linux is compile-checked + unit-tested parsers only.
- `manufacture_date` is always null (drives rarely report it).
- No end-to-end run against a real external drive yet (needs admin).
