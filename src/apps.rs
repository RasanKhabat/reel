//! Which running apps Reel manages.

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

/// Whether Reel should manage this app's windows. In test mode: only the test host.
/// Otherwise: every regular (Dock) app except Reel itself and the test host.
pub fn manages(app: &NSRunningApplication) -> bool {
    let pid = app.processIdentifier();
    if pid == std::process::id() as i32 {
        return false;
    }
    if let Some(test_pid) = crate::test_mode::pid() {
        return pid == test_pid;
    }
    if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
        return false;
    }
    let exe = app.executableURL().and_then(|u| u.lastPathComponent()).map(|s| s.to_string());
    exe.as_deref() != Some(crate::test_mode::HOST_NAME)
}

/// `manages` for a pid (false if the process is gone).
pub fn manages_pid(pid: i32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid).is_some_and(|a| manages(&a))
}

pub fn managed() -> Vec<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace().runningApplications().iter().filter(|a| manages(a)).collect()
}

pub struct AppInfo {
    pub name: String,
    pub bundle_id: Option<String>,
}

pub fn info(pid: i32) -> AppInfo {
    match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        Some(a) => AppInfo {
            name: a.localizedName().map(|s| s.to_string()).unwrap_or_else(|| format!("pid {pid}")),
            bundle_id: a.bundleIdentifier().map(|s| s.to_string()),
        },
        None => AppInfo { name: format!("pid {pid}"), bundle_id: None },
    }
}
