//! "Launch at Login" through SMAppService (macOS 13+): a standard login item, listed in
//! System Settings → General → Login Items where it can also be turned off.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

fn class() -> Option<&'static AnyClass> {
    AnyClass::get(c"SMAppService")
}

fn service() -> Option<Retained<AnyObject>> {
    let cls = class()?;
    unsafe { msg_send![cls, mainAppService] }
}

fn status() -> isize {
    // 0 not registered, 1 enabled, 2 requires approval, 3 not found.
    service().map(|s| unsafe { msg_send![&*s, status] }).unwrap_or(0)
}

pub fn is_enabled() -> bool {
    status() == 1
}

pub fn toggle() {
    let Some(s) = service() else { return };
    let enable = !is_enabled();
    let ok: bool = unsafe {
        if enable {
            msg_send![&*s, registerAndReturnError: std::ptr::null_mut::<*mut AnyObject>()]
        } else {
            msg_send![&*s, unregisterAndReturnError: std::ptr::null_mut::<*mut AnyObject>()]
        }
    };
    crate::log::write(&format!("launch at login {} → ok {ok}, status {}", if enable { "on" } else { "off" }, status()));
    if status() == 2 {
        if let Some(cls) = class() {
            let _: () = unsafe { msg_send![cls, openSystemSettingsLoginItems] };
        }
    }
}
