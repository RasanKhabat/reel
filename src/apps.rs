//! Which running apps Reel manages.

use crate::ax::{self, Element};
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

/// Minimizes the given app's front window (whether or not the app is active).
pub fn minimize_front_window(bundle_id: &str) {
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    let Some(app) = apps.iter().find(|a| a.bundleIdentifier().is_some_and(|b| b.to_string() == bundle_id)) else { return };
    let el = Element::application(app.processIdentifier());
    el.set_timeout(0.5);
    let win = el
        .element(ax::FOCUSED_WINDOW)
        .or_else(|| el.element(ax::MAIN_WINDOW))
        .filter(|w| !w.is_minimized())
        .or_else(|| el.windows().into_iter().find(|w| w.subrole().as_deref() == Some(ax::STANDARD_WINDOW) && !w.is_minimized()));
    if let Some(w) = win {
        w.set_bool(ax::MINIMIZED, true);
    }
}
