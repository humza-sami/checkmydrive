//! clap CLI. stdout is JSON only; logs go to stderr.

use crate::engine::{self, EngineError, TestOptions, EXIT_BAD_ARGS, EXIT_INTERNAL, EXIT_OK};
use crate::progress::Emitter;
use crate::report::Mode;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "checkmydrive-engine", about = "CheckMyDrive disk-testing engine (JSON on stdout)", disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
    Safe,
    Wipe,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// List drives as a JSON array of DriveInfo (works without admin rights).
    List,
    /// Test a drive; emits NDJSON progress then one report (or error) line.
    Test {
        #[arg(long)]
        drive: String,
        #[arg(long, value_enum)]
        mode: ModeArg,
        /// Required in wipe mode: the drive's serial (or its id if it reports no serial).
        #[arg(long)]
        confirm_serial: Option<String>,
        /// Also append every output line to this file.
        #[arg(long)]
        out: Option<String>,
    },
    /// Print engine and smartctl versions as JSON.
    Version,
}

fn print_json(v: &serde_json::Value) {
    println!("{}", v);
}

pub fn error_line(e: &EngineError) -> serde_json::Value {
    let mut v = json!({"type": "error", "code": e.code});
    if let Some(p) = &e.params {
        v["params"] = json!(p);
    }
    v
}

/// Entry point; returns the process exit code.
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind;
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = e.print();
                return EXIT_OK;
            }
            eprintln!("{}", e);
            print_json(&json!({"type": "error", "code": "error.bad_args"}));
            return EXIT_BAD_ARGS;
        }
    };
    match cli.cmd {
        Cmd::Version => {
            print_json(&engine::version_json());
            EXIT_OK
        }
        Cmd::List => {
            let drives = engine::list_enriched();
            match serde_json::to_string(&drives) {
                Ok(s) => {
                    println!("{}", s);
                    EXIT_OK
                }
                Err(_) => EXIT_INTERNAL,
            }
        }
        Cmd::Test { drive, mode, confirm_serial, out } => {
            let emit = match Emitter::new(out.as_deref().map(std::path::Path::new)) {
                Ok(e) => Arc::new(e),
                Err(err) => {
                    eprintln!("cannot open --out file: {}", err);
                    print_json(&json!({"type": "error", "code": "error.bad_args"}));
                    return EXIT_BAD_ARGS;
                }
            };
            // Ctrl-C / SIGTERM: delete any temp file, then leave.
            let _ = ctrlc::set_handler(|| {
                crate::cleanup::cleanup_all();
                std::process::exit(130);
            });
            let opts = TestOptions {
                drive,
                mode: match mode {
                    ModeArg::Safe => Mode::Safe,
                    ModeArg::Wipe => Mode::Wipe,
                },
                confirm_serial,
            };
            let e2 = emit.clone();
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine::run_test(&opts, e2)));
            crate::cleanup::cleanup_all();
            match res {
                Ok(Ok(report)) => {
                    emit.line(&json!({"type": "report", "report": report}));
                    EXIT_OK
                }
                Ok(Err(err)) => {
                    emit.line(&error_line(&err));
                    err.exit
                }
                Err(_) => {
                    emit.line(&json!({"type": "error", "code": "error.internal"}));
                    EXIT_INTERNAL
                }
            }
        }
    }
}
