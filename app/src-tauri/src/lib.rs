//! CheckMyDrive desktop shell.
//!
//! Talks to the `checkmydrive-engine` sidecar exactly as described in docs/CONTRACT.md:
//! `list` -> JSON array, `test` -> NDJSON stream. Elevation per OS:
//!   * macOS:   osascript "with administrator privileges", engine writes `--out <tmp>`, we tail it.
//!   * Linux:   pkexec, we read stdout.
//!   * Windows: the app itself is elevated through its manifest; spawn directly.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(windows)]
use tokio::io::{AsyncBufReadExt, BufReader};
#[cfg(not(windows))]
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::process::Command;

const EVENT: &str = "engine-event";

#[derive(Default)]
struct TestState {
    /// Path of the cancel-flag file for the running test (macOS/Linux), or None.
    cancel_file: Mutex<Option<PathBuf>>,
    /// PID of a directly spawned engine (Windows).
    direct_pid: Mutex<Option<u32>>,
    running: Mutex<bool>,
}

/// Locate the engine binary: next to the app executable (bundled sidecar, triple stripped),
/// or `src-tauri/binaries/checkmydrive-engine-<triple>` during development.
fn engine_path() -> Result<PathBuf, String> {
    let exe_name = if cfg!(windows) { "checkmydrive-engine.exe" } else { "checkmydrive-engine" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(exe_name);
            if p.exists() {
                return Ok(p);
            }
            // macOS .app: Contents/MacOS/<bin>; Linux deb: same dir as exe.
            for sub in ["resources", "../Resources"] {
                let p = dir.join(sub).join(exe_name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }
    if let Ok(dir) = std::env::var("CHECKMYDRIVE_ENGINE") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Ok(p);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    if let Ok(rd) = std::fs::read_dir(&dev) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("checkmydrive-engine-") && e.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                return Ok(e.path());
            }
        }
    }
    Err("engine_missing".into())
}

/// Bundled smartctl: `<resource_dir>/smartctl[.exe]` (macOS: Contents/Resources, Windows: install dir).
/// Linux .deb packages depend on the distro's `smartmontools` instead, so nothing is found here
/// and the engine falls back to /usr/sbin.
fn bundled_smartctl(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) { "smartctl.exe" } else { "smartctl" };
    let dir = app.path().resource_dir().ok()?;
    [dir.join(name), dir.join("resources").join(name)].into_iter().find(|p| p.is_file())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[tauri::command]
async fn engine_available() -> bool {
    engine_path().is_ok()
}

#[tauri::command]
async fn list_drives(app: AppHandle) -> Result<Value, String> {
    list_drives_impl(engine_path()?, bundled_smartctl(&app)).await
}

async fn list_drives_impl(engine: PathBuf, smartctl: Option<PathBuf>) -> Result<Value, String> {
    let mut cmd = Command::new(engine);
    if let Some(sc) = smartctl {
        cmd.env("CHECKMYDRIVE_SMARTCTL", sc);
    }
    let out = cmd
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("engine_spawn_failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "engine_failed: exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("engine_bad_output: {e}"))
}

fn emit_line(app: &AppHandle, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(v) => {
            let ty = v.get("type").and_then(|t| t.as_str()).map(|s| s.to_string());
            let _ = app.emit(EVENT, v);
            ty
        }
        Err(_) => None,
    }
}

fn fail(app: &AppHandle, code: &str) {
    let _ = app.emit(EVENT, json!({ "type": "error", "code": code }));
}

fn exit_code_to_error(code: Option<i32>) -> &'static str {
    match code {
        Some(3) => "error.needs_admin",
        Some(4) => "error.drive_not_found",
        Some(5) => "error.refused",
        Some(2) => "error.bad_args",
        _ => "error.engine_failed",
    }
}

#[tauri::command]
async fn start_test(
    app: AppHandle,
    state: State<'_, TestState>,
    drive_id: String,
    mode: String,
    confirm_serial: Option<String>,
) -> Result<(), String> {
    if mode != "safe" && mode != "wipe" {
        return Err("bad mode".into());
    }
    {
        let mut r = state.running.lock().unwrap();
        if *r {
            return Err("test already running".into());
        }
        *r = true;
    }
    let engine = match engine_path() {
        Ok(p) => p,
        Err(e) => {
            *state.running.lock().unwrap() = false;
            return Err(e);
        }
    };

    let mut args: Vec<String> = vec![
        "test".into(),
        "--drive".into(),
        drive_id,
        "--mode".into(),
        mode,
    ];
    if let Some(s) = confirm_serial {
        args.push("--confirm-serial".into());
        args.push(s);
    }

    let smartctl = bundled_smartctl(&app);
    let app2 = app.clone();
    tokio::spawn(async move {
        let result = run_engine(&app2, engine, args, smartctl).await;
        let st = app2.state::<TestState>();
        *st.running.lock().unwrap() = false;
        *st.cancel_file.lock().unwrap() = None;
        *st.direct_pid.lock().unwrap() = None;
        if let Err(code) = result {
            fail(&app2, &code);
        }
    });
    Ok(())
}

#[cfg(windows)]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt as _;

#[cfg(windows)]
async fn run_engine(app: &AppHandle, engine: PathBuf, args: Vec<String>, smartctl: Option<PathBuf>) -> Result<(), String> {
    let mut cmd = Command::new(engine);
    if let Some(sc) = &smartctl {
        cmd.env("CHECKMYDRIVE_SMARTCTL", sc);
    }
    let mut child = cmd
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|_| "error.engine_failed".to_string())?;
    *app.state::<TestState>().direct_pid.lock().unwrap() = child.id();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let mut terminal = false;
    while let Ok(Some(l)) = lines.next_line().await {
        if matches!(emit_line(app, &l).as_deref(), Some("report") | Some("error")) {
            terminal = true;
        }
    }
    let status = child.wait().await.map_err(|_| "error.engine_failed".to_string())?;
    if !terminal {
        if app.state::<TestState>().cancel_file.lock().unwrap().is_some() {
            let _ = app.emit(EVENT, json!({"type":"cancelled"}));
            return Ok(());
        }
        return Err(exit_code_to_error(status.code()).into());
    }
    Ok(())
}

#[cfg(not(windows))]
async fn run_engine(app: &AppHandle, engine: PathBuf, args: Vec<String>, smartctl: Option<PathBuf>) -> Result<(), String> {
    let tmp = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let cancel = tmp.join(format!("checkmydrive-cancel-{stamp}"));
    let out_file = tmp.join(format!("checkmydrive-out-{stamp}.ndjson"));
    let _ = std::fs::remove_file(&cancel);
    let _ = std::fs::write(&out_file, b"");
    *app.state::<TestState>().cancel_file.lock().unwrap() = Some(cancel.clone());

    // Elevated wrapper: run the engine in the background and watch for the cancel flag file,
    // because a non-root GUI cannot signal a root process.
    let quoted_args: Vec<String> = args.iter().map(|a| sh_quote(a)).collect();
    let out_arg = if cfg!(target_os = "macos") {
        format!(" --out {}", sh_quote(&out_file.to_string_lossy()))
    } else {
        String::new()
    };
    // The elevated shell does not inherit our environment, so pass smartctl's path inline.
    let env_prefix = smartctl
        .map(|p| format!("CHECKMYDRIVE_SMARTCTL={} ", sh_quote(&p.to_string_lossy())))
        .unwrap_or_default();
    let script = format!(
        "{env}{eng} {args}{out} {redir} & pid=$!; \
         while kill -0 $pid 2>/dev/null; do \
           if [ -e {cancel} ]; then kill $pid 2>/dev/null; fi; sleep 0.5; done; \
         wait $pid",
        env = env_prefix,
        eng = sh_quote(&engine.to_string_lossy()),
        args = quoted_args.join(" "),
        out = out_arg,
        redir = if cfg!(target_os = "macos") { ">/dev/null 2>&1" } else { "2>/dev/null" },
        cancel = sh_quote(&cancel.to_string_lossy()),
    );

    #[cfg(target_os = "macos")]
    let mut cmd = {
        let esc = script.replace('\\', "\\\\").replace('"', "\\\"");
        let mut c = Command::new("osascript");
        c.arg("-e").arg(format!(
            "do shell script \"{esc}\" with administrator privileges"
        ));
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = {
        let mut c = Command::new("pkexec");
        c.arg("/bin/sh").arg("-c").arg(&script);
        c
    };

    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|_| "error.elevation_unavailable".to_string())?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let mut terminal = false;
    let mut pos: u64 = 0;
    let mut pending = String::new();
    let mut stdout_buf = String::new();

    let status = loop {
        // macOS: tail the --out file. Linux: stdout lines.
        if cfg!(target_os = "macos") {
            if let Ok(mut f) = tokio::fs::File::open(&out_file).await {
                if f.seek(std::io::SeekFrom::Start(pos)).await.is_ok() {
                    let mut chunk = Vec::new();
                    if let Ok(n) = f.read_to_end(&mut chunk).await {
                        pos += n as u64;
                        pending.push_str(&String::from_utf8_lossy(&chunk));
                    }
                }
            }
            while let Some(i) = pending.find('\n') {
                let line: String = pending.drain(..=i).collect();
                if matches!(emit_line(app, &line).as_deref(), Some("report") | Some("error")) {
                    terminal = true;
                }
            }
        } else {
            let mut chunk = [0u8; 8192];
            if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(200), stdout.read(&mut chunk)).await {
                if n > 0 {
                    stdout_buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
                }
            }
            while let Some(i) = stdout_buf.find('\n') {
                let line: String = stdout_buf.drain(..=i).collect();
                if matches!(emit_line(app, &line).as_deref(), Some("report") | Some("error")) {
                    terminal = true;
                }
            }
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => tokio::time::sleep(Duration::from_millis(250)).await,
            Err(_) => return Err("error.engine_failed".into()),
        }
    };

    // Drain whatever is left.
    if cfg!(target_os = "macos") {
        if let Ok(rest) = tokio::fs::read(&out_file).await {
            let tail = String::from_utf8_lossy(&rest[(pos as usize).min(rest.len())..]).to_string();
            pending.push_str(&tail);
        }
        for l in pending.lines() {
            if matches!(emit_line(app, l).as_deref(), Some("report") | Some("error")) {
                terminal = true;
            }
        }
    } else {
        let mut rest = Vec::new();
        let _ = stdout.read_to_end(&mut rest).await;
        stdout_buf.push_str(&String::from_utf8_lossy(&rest));
        for l in stdout_buf.lines() {
            if matches!(emit_line(app, l).as_deref(), Some("report") | Some("error")) {
                terminal = true;
            }
        }
    }
    let mut err = String::new();
    let _ = stderr.read_to_string(&mut err).await;

    let cancelled = cancel.exists();
    let _ = std::fs::remove_file(&cancel);
    let _ = std::fs::remove_file(&out_file);

    if terminal {
        return Ok(());
    }
    if cancelled {
        let _ = app.emit(EVENT, json!({"type":"cancelled"}));
        return Ok(());
    }
    let lower = err.to_lowercase();
    if lower.contains("user canceled") || lower.contains("-128") || lower.contains("dismissed") || status.code() == Some(126) {
        return Err("error.auth_cancelled".into());
    }
    Err(exit_code_to_error(status.code()).into())
}

#[tauri::command]
async fn cancel_test(state: State<'_, TestState>) -> Result<(), String> {
    if let Some(p) = state.cancel_file.lock().unwrap().clone() {
        let _ = std::fs::write(p, b"1");
    }
    #[cfg(windows)]
    if let Some(pid) = *state.direct_pid.lock().unwrap() {
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
    }
    Ok(())
}

#[tauri::command]
async fn save_text_file(path: String, contents: String) -> Result<(), String> {
    tokio::fs::write(path, contents).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TestState::default())
        .invoke_handler(tauri::generate_handler![
            engine_available,
            list_drives,
            start_test,
            cancel_test,
            save_text_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running CheckMyDrive");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Non-mock path: spawn the real engine exactly as `list_drives` does and parse its JSON.
    /// Uses the sidecar built by scripts/build-sidecar.sh, or engine/target/{release,debug}.
    #[tokio::test]
    async fn list_drives_returns_real_data_from_engine() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let exe = if cfg!(windows) { "checkmydrive-engine.exe" } else { "checkmydrive-engine" };
        let mut cands: Vec<PathBuf> = vec![
            root.join("../../engine/target/release").join(exe),
            root.join("../../engine/target/debug").join(exe),
        ];
        if let Ok(rd) = std::fs::read_dir(root.join("binaries")) {
            cands.extend(rd.flatten().map(|e| e.path()));
        }
        let engine = cands
            .into_iter()
            .find(|p| p.is_file() && p.metadata().map(|m| m.len() > 0).unwrap_or(false))
            .expect("no engine binary: run scripts/build-sidecar.sh or `cargo build` in engine/");
        let smartctl = Some(root.join("resources").join(if cfg!(windows) { "smartctl.exe" } else { "smartctl" }));
        let v = list_drives_impl(engine, smartctl).await.expect("engine list failed");
        let drives = v.as_array().expect("list must be a JSON array");
        assert!(!drives.is_empty(), "expected at least one drive");
        for d in drives {
            assert!(d["id"].is_string() && d["capacity_bytes"].is_number() && d["kind"].is_string(), "bad DriveInfo: {d}");
        }
        assert!(drives.iter().any(|d| d["is_system_disk"] == true), "no system disk reported");
    }
}
