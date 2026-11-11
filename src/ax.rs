//! Safe-ish wrapper over AXUIElement.

use crate::ffi::*;
use std::ffi::c_void;

pub const ROLE: &str = "AXRole";
pub const SUBROLE: &str = "AXSubrole";
pub const TITLE: &str = "AXTitle";
pub const WINDOWS: &str = "AXWindows";
pub const POSITION: &str = "AXPosition";
pub const SIZE: &str = "AXSize";
pub const MINIMIZED: &str = "AXMinimized";
pub const FULL_SCREEN: &str = "AXFullScreen";
pub const MAIN: &str = "AXMain";
pub const FRONTMOST: &str = "AXFrontmost";
pub const FOCUSED_WINDOW: &str = "AXFocusedWindow";
pub const MAIN_WINDOW: &str = "AXMainWindow";
pub const FOCUSED_APPLICATION: &str = "AXFocusedApplication";
pub const WINDOW_ROLE: &str = "AXWindow";
pub const STANDARD_WINDOW: &str = "AXStandardWindow";
pub const RAISE: &str = "AXRaise";

/// A retained AXUIElement. AX calls are thread-safe, so it may cross threads.
pub struct Element(AXUIElementRef);

unsafe impl Send for Element {}
unsafe impl Sync for Element {}

impl Clone for Element {
    fn clone(&self) -> Self {
        unsafe { CFRetain(self.0) };
        Element(self.0)
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        unsafe { CFEqual(self.0, other.0) != 0 }
    }
}
impl Eq for Element {}

impl std::hash::Hash for Element {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe { CFHash(self.0) }.hash(state)
    }
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Shows the system "allow Accessibility" prompt if not yet trusted.
pub fn request_trust() {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let dict = Cf(CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const c_void,
        ));
        AXIsProcessTrustedWithOptions(dict.0);
    }
}

impl Element {
    /// Takes a +1 reference.
    pub fn owned(r: AXUIElementRef) -> Option<Element> {
        if r.is_null() { None } else { Some(Element(r)) }
    }

    /// Retains a borrowed reference.
    pub fn retained(r: AXUIElementRef) -> Option<Element> {
        if r.is_null() {
            return None;
        }
        unsafe { CFRetain(r) };
        Some(Element(r))
    }

    pub fn application(pid: i32) -> Element {
        Element(unsafe { AXUIElementCreateApplication(pid) })
    }

    pub fn system_wide() -> Element {
        Element(unsafe { AXUIElementCreateSystemWide() })
    }

    pub fn raw(&self) -> AXUIElementRef {
        self.0
    }

    pub fn set_timeout(&self, seconds: f32) {
        unsafe { AXUIElementSetMessagingTimeout(self.0, seconds) };
    }

    pub fn pid(&self) -> i32 {
        let mut pid = 0;
        unsafe { AXUIElementGetPid(self.0, &mut pid) };
        pid
    }

    /// Runs one AX request, unless this is the main thread and the app recently failed to
    /// answer in time: then it fails at once instead of waiting out the timeout again.
    fn call(&self, f: impl FnOnce() -> AXError) -> AXError {
        let pid = self.pid();
        if pid > 0 && unresponsive::skip(pid) {
            return kAXErrorCannotComplete;
        }
        let start = std::time::Instant::now();
        let err = f();
        // The same error also comes back at once for some refusals; only a request that
        // waited (most of) its timeout means the app is busy. Normal answers take a few
        // milliseconds; Reel's shortest timeout is 0.1 s.
        if err == kAXErrorCannotComplete && pid > 0 && start.elapsed() >= unresponsive::SLOW {
            unresponsive::mark(pid);
        }
        err
    }

    fn copy(&self, attr: &str) -> Result<Cf, AXError> {
        let name = cfstr(attr);
        let mut out: CFTypeRef = std::ptr::null();
        let err = self.call(|| unsafe { AXUIElementCopyAttributeValue(self.0, name.0, &mut out) });
        if err != kAXErrorSuccess {
            return Err(err);
        }
        Cf::owned(out).ok_or(err)
    }

    pub fn string(&self, attr: &str) -> Option<String> {
        self.copy(attr).ok().and_then(|v| string_from_cf(v.0))
    }

    pub fn bool(&self, attr: &str) -> Option<bool> {
        let v = self.copy(attr).ok()?;
        unsafe {
            if CFGetTypeID(v.0) == CFBooleanGetTypeID() { Some(CFBooleanGetValue(v.0) != 0) } else { None }
        }
    }

    pub fn element(&self, attr: &str) -> Option<Element> {
        let v = self.copy(attr).ok()?;
        let r = v.0;
        std::mem::forget(v); // ownership moves into the Element
        Element::owned(r)
    }

    pub fn elements(&self, attr: &str) -> Vec<Element> {
        let Ok(v) = self.copy(attr) else { return vec![] };
        unsafe {
            if CFGetTypeID(v.0) != CFArrayGetTypeID() {
                return vec![];
            }
            (0..CFArrayGetCount(v.0)).filter_map(|i| Element::retained(CFArrayGetValueAtIndex(v.0, i))).collect()
        }
    }

    pub fn role(&self) -> Option<String> {
        self.string(ROLE)
    }
    pub fn subrole(&self) -> Option<String> {
        self.string(SUBROLE)
    }
    pub fn title(&self) -> String {
        self.string(TITLE).unwrap_or_default()
    }
    pub fn windows(&self) -> Vec<Element> {
        self.elements(WINDOWS)
    }
    pub fn is_minimized(&self) -> bool {
        self.bool(MINIMIZED).unwrap_or(false)
    }

    pub fn position(&self) -> Option<CGPoint> {
        let v = self.copy(POSITION).ok()?;
        let mut p = CGPoint::default();
        unsafe { (CFGetTypeID(v.0) == AXValueGetTypeID() && AXValueGetValue(v.0, kAXValueCGPointType, &mut p as *mut _ as *mut c_void) != 0).then_some(p) }
    }

    pub fn size(&self) -> Option<CGSize> {
        let v = self.copy(SIZE).ok()?;
        let mut s = CGSize::default();
        unsafe { (CFGetTypeID(v.0) == AXValueGetTypeID() && AXValueGetValue(v.0, kAXValueCGSizeType, &mut s as *mut _ as *mut c_void) != 0).then_some(s) }
    }

    /// Frame in AX (top-left origin, global) coordinates.
    pub fn frame(&self) -> Option<crate::geometry::Rect> {
        let p = self.position()?;
        let s = self.size()?;
        Some(crate::geometry::Rect::new(p.x, p.y, s.width, s.height))
    }

    pub fn set_position(&self, x: f64, y: f64) {
        let p = CGPoint { x, y };
        unsafe {
            if let Some(v) = Cf::owned(AXValueCreate(kAXValueCGPointType, &p as *const _ as *const c_void)) {
                let name = cfstr(POSITION);
                self.call(|| AXUIElementSetAttributeValue(self.0, name.0, v.0));
            }
        }
    }

    pub fn set_size(&self, width: f64, height: f64) {
        let s = CGSize { width, height };
        unsafe {
            if let Some(v) = Cf::owned(AXValueCreate(kAXValueCGSizeType, &s as *const _ as *const c_void)) {
                let name = cfstr(SIZE);
                self.call(|| AXUIElementSetAttributeValue(self.0, name.0, v.0));
            }
        }
    }

    pub fn set_bool(&self, attr: &str, value: bool) {
        let name = cfstr(attr);
        self.call(|| unsafe { AXUIElementSetAttributeValue(self.0, name.0, if value { kCFBooleanTrue } else { kCFBooleanFalse }) });
    }

    pub fn perform(&self, action: &str) {
        let name = cfstr(action);
        self.call(|| unsafe { AXUIElementPerformAction(self.0, name.0) });
    }

    /// True only when AX says the element no longer exists. A busy app that times out
    /// also returns no frame, so "frame unreadable" alone must not count as closed.
    pub fn is_gone(&self) -> bool {
        matches!(self.copy(ROLE), Err(e) if e == kAXErrorInvalidUIElement)
    }

    /// Tiled windows: real, standard, visible, not full screen.
    pub fn is_tileable(&self) -> bool {
        self.role().as_deref() == Some(WINDOW_ROLE)
            && self.subrole().as_deref() == Some(STANDARD_WINDOW)
            && !self.is_minimized()
            && !self.bool(FULL_SCREEN).unwrap_or(false)
    }

    /// The window with keyboard focus system-wide, if any.
    pub fn system_focused_window() -> Option<Element> {
        Element::system_wide().element(FOCUSED_APPLICATION)?.element(FOCUSED_WINDOW)
    }
}

/// Whether the app recently failed to answer in time (Reel is leaving it alone).
pub fn app_unresponsive(pid: i32) -> bool {
    unresponsive::is_busy(pid)
}

/// Apps that recently didn't answer an AX request in time. Every request on the main thread
/// to a busy app would wait out the full timeout, and a single command makes many of them
/// (one per window), which froze Reel's keys and menu for seconds. For a short while after
/// a timeout, main-thread requests to that app fail at once. The mover threads still try
/// (each app has its own), and their timeouts count too.
mod unresponsive {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const BACK_OFF: Duration = Duration::from_secs(3);
    pub const SLOW: Duration = Duration::from_millis(90);
    static UNTIL: Mutex<Option<HashMap<i32, Instant>>> = Mutex::new(None);

    fn on_main_thread() -> bool {
        unsafe { libc::pthread_main_np() == 1 }
    }

    pub fn skip(pid: i32) -> bool {
        on_main_thread() && is_busy(pid)
    }

    pub fn is_busy(pid: i32) -> bool {
        let guard = UNTIL.lock().unwrap();
        guard.as_ref().and_then(|m| m.get(&pid)).is_some_and(|t| Instant::now() < *t)
    }

    pub fn mark(pid: i32) {
        let mut guard = UNTIL.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        let now = Instant::now();
        let was_busy = map.get(&pid).is_some_and(|t| now < *t);
        map.insert(pid, now + BACK_OFF);
        drop(guard);
        if !was_busy {
            let name = if on_main_thread() { crate::apps::info(pid).name } else { format!("pid {pid}") };
            crate::log::write(&format!("{name} isn't answering; leaving it alone for a moment"));
        }
    }
}
