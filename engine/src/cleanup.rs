//! Temp-file registry + RAII guard so safe mode never leaves files behind
//! (normal exit, early return, panic unwinding, Ctrl-C / SIGTERM via `cleanup_all`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static REGISTRY: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Remove every registered temp file. Safe to call from a signal-handler thread.
pub fn cleanup_all() {
    let paths: Vec<PathBuf> = match REGISTRY.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(p) => std::mem::take(&mut *p.into_inner()),
    };
    for p in paths {
        let _ = std::fs::remove_file(&p);
    }
}

pub struct TempGuard {
    path: PathBuf,
}

impl TempGuard {
    /// Register `path` for deletion. Call BEFORE creating the file.
    pub fn new(path: &Path) -> TempGuard {
        if let Ok(mut g) = REGISTRY.lock() {
            g.push(path.to_path_buf());
        }
        TempGuard { path: path.to_path_buf() }
    }

    /// Delete now; returns false if the file still exists afterwards.
    pub fn remove(&self) -> bool {
        match std::fs::remove_file(&self.path) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        if let Ok(mut g) = REGISTRY.lock() {
            g.retain(|p| p != &self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guard_deletes_on_drop_and_cleanup_all() {
        let d = std::env::temp_dir();
        let a = d.join(format!(".cmd-test-a-{}", crate::util::rand_hex(8)));
        let b = d.join(format!(".cmd-test-b-{}", crate::util::rand_hex(8)));
        {
            let _g = TempGuard::new(&a);
            std::fs::write(&a, b"x").unwrap();
        }
        assert!(!a.exists());
        let _g2 = TempGuard::new(&b);
        std::fs::write(&b, b"x").unwrap();
        cleanup_all();
        assert!(!b.exists());
    }
}
