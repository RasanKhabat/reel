//! Recovers from a Force Quit. An app gets no chance to run code when it's force-quit
//! (SIGKILL), so windows Reel had parked off screen would stay there. Reel therefore starts
//! a tiny `sh` loop that outlives it: if Reel disappears *without* leaving the "clean exit"
//! marker, the loop reopens Reel, which puts the windows back.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn clean_exit_marker() -> PathBuf {
    crate::support_dir().join("clean-exit")
}

fn last_recovery() -> PathBuf {
    crate::support_dir().join("last-recovery")
}

/// True when this launch was started by the watchdog after a force quit.
pub fn is_recovery_launch() -> bool {
    std::env::args().any(|a| a == "--recover")
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The .app bundle containing this executable, if any.
fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(PathBuf::from)
}

pub fn start() {
    let _ = std::fs::create_dir_all(crate::support_dir());
    let _ = std::fs::remove_file(clean_exit_marker());

    // Don't loop forever if Reel keeps dying right after launch.
    if is_recovery_launch() {
        let last = std::fs::read_to_string(last_recovery()).ok().and_then(|s| s.trim().parse::<f64>().ok());
        if last.is_some_and(|t| now() - t < 30.0) {
            crate::log::write("watchdog: recovered twice within 30 s; not arming again");
            return;
        }
        let _ = std::fs::write(last_recovery(), now().to_string());
    }

    let Some(app) = bundle_path() else { return };
    // $1 pid, $2 clean-exit marker, $3 app, $4 open flag ("-n" in test mode), rest: args.
    let script = r#"pid="$1"; marker="$2"; app="$3"; flag="$4"; shift 4
while kill -0 "$pid" 2>/dev/null; do sleep 0.5; done
if [ -f "$marker" ]; then rm -f "$marker"; exit 0; fi
sleep 0.3
/usr/bin/open $flag -g "$app" --args --recover "$@""#;
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(script)
        .arg("reel-watchdog")
        .arg(std::process::id().to_string())
        .arg(clean_exit_marker())
        .arg(&app)
        .arg(if crate::test_mode::is_on() { "-n" } else { "" })
        .args(crate::test_mode::relaunch_arguments())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(child) => crate::log::write(&format!("watchdog armed (pid {})", child.id())),
        Err(e) => crate::log::write(&format!("watchdog could not start: {e}")),
    }
}

/// Called on every orderly exit so the watchdog stands down.
pub fn mark_clean_exit() {
    let _ = std::fs::write(clean_exit_marker(), b"");
}
