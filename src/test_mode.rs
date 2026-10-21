//! Automated-test mode: `--test-pid <pid> --test-dir <dir>`. Reel then manages only the
//! test host's windows, registers no shortcuts or gestures, shows no menu bar icon, keeps
//! its files in the test directory and takes commands over a distributed notification.
//! A normal Reel ignores the test host entirely, so tests never touch real windows.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const HOST_NAME: &str = "ReelTestHost";
pub const COMMAND_NOTIFICATION: &str = "local.reel.wm.test.command";

fn argument(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

pub fn pid() -> Option<i32> {
    static PID: OnceLock<Option<i32>> = OnceLock::new();
    *PID.get_or_init(|| argument("--test-pid").and_then(|s| s.parse().ok()))
}

pub fn directory() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| argument("--test-dir").map(PathBuf::from)).as_ref()
}

pub fn is_on() -> bool {
    pid().is_some() && directory().is_some()
}

/// Arguments a watchdog relaunch must carry so the new instance is a test instance too.
pub fn relaunch_arguments() -> Vec<String> {
    match (pid(), directory()) {
        (Some(p), Some(d)) => vec!["--test-pid".into(), p.to_string(), "--test-dir".into(), d.display().to_string()],
        _ => vec![],
    }
}
