//! NDJSON emitter with overall-percent bookkeeping and a 1 s heartbeat.

use crate::report::*;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Emitter {
    out_file: Mutex<Option<File>>,
    stdout_lock: Mutex<()>,
    last: Mutex<Option<(ProgressEvent, Instant)>>,
    overall: Mutex<f64>,
}

/// Overall-percent window for a phase.
pub fn phase_range(p: Phase) -> (f64, f64) {
    match p {
        Phase::Prepare => (0.0, 4.0),
        Phase::DriveInfo => (4.0, 6.0),
        Phase::SmartHealth => (6.0, 9.0),
        Phase::SurfaceRead => (9.0, 55.0),
        Phase::SelftestShort => (9.0, 70.0),
        Phase::WriteVerify => (70.0, 86.0),
        Phase::CapacityCheck => (86.0, 95.0),
        Phase::Stability => (95.0, 99.0),
        Phase::Finish => (99.0, 100.0),
    }
}

impl Emitter {
    pub fn new(out: Option<&Path>) -> std::io::Result<Self> {
        let f = match out {
            Some(p) => Some(OpenOptions::new().create(true).append(true).open(p)?),
            None => None,
        };
        Ok(Emitter {
            out_file: Mutex::new(f),
            stdout_lock: Mutex::new(()),
            last: Mutex::new(None),
            overall: Mutex::new(0.0),
        })
    }

    /// Write one JSON value as a line to stdout (and the --out file).
    pub fn line<T: Serialize>(&self, v: &T) {
        let Ok(s) = serde_json::to_string(v) else { return };
        let _g = self.stdout_lock.lock().unwrap();
        let stdout = std::io::stdout();
        let mut h = stdout.lock();
        let _ = writeln!(h, "{}", s);
        let _ = h.flush();
        if let Some(f) = self.out_file.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{}", s);
            let _ = f.flush();
        }
    }

    pub fn progress(&self, phase: Phase, phase_percent: f64, code: &str, params: Option<Params>) {
        let pp = phase_percent.clamp(0.0, 100.0);
        let (lo, hi) = phase_range(phase);
        let want = lo + (hi - lo) * pp / 100.0;
        // Hold the `overall` lock across the write so frames from concurrent phases
        // (selftest_short + surface_read) reach the output in monotonic order.
        let mut o = self.overall.lock().unwrap();
        if want > *o {
            *o = want;
        }
        let percent = *o;
        let ev = ProgressEvent {
            kind: "progress".into(),
            phase,
            percent: (percent * 10.0).round() / 10.0,
            phase_percent: (pp * 10.0).round() / 10.0,
            message_code: code.to_string(),
            params,
        };
        self.line(&ev);
        *self.last.lock().unwrap() = Some((ev, Instant::now()));
    }

    /// Re-emit the last frame if nothing was emitted for ~1 s.
    pub fn heartbeat(&self) {
        let ev = {
            let l = self.last.lock().unwrap();
            match l.as_ref() {
                Some((e, t)) if t.elapsed() >= Duration::from_millis(900) => Some(e.clone()),
                _ => None,
            }
        };
        if let Some(e) = ev {
            self.line(&e);
            if let Some((_, t)) = self.last.lock().unwrap().as_mut() {
                *t = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// selftest_short and surface_read report from two threads at once; the overall percent
    /// that reaches the GUI must never go backwards.
    #[test]
    fn overall_percent_is_monotonic_with_concurrent_phases() {
        let path = std::env::temp_dir().join(format!("cmd-progress-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let em = Arc::new(Emitter::new(Some(&path)).unwrap());
        let a = {
            let em = em.clone();
            std::thread::spawn(move || {
                for i in 0..=100 {
                    em.progress(Phase::SurfaceRead, i as f64, "progress.surface_read.sampling", None);
                }
            })
        };
        let b = {
            let em = em.clone();
            std::thread::spawn(move || {
                for i in 0..=100 {
                    em.progress(Phase::SelftestShort, i as f64, "progress.selftest_short.running", None);
                }
            })
        };
        a.join().unwrap();
        b.join().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let mut last = 0.0;
        let mut n = 0;
        for l in text.lines() {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            let p = v["percent"].as_f64().unwrap();
            assert!(p + 1e-9 >= last, "percent went backwards: {last} -> {p}");
            last = p;
            n += 1;
        }
        assert_eq!(n, 202);
    }
}
