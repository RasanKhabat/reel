//! Rectangles and displays. Reel works in Accessibility coordinates (top-left origin of the
//! primary display); AppKit uses bottom-left. Conversions live here.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSNumber, NSRect, NSString};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }
    pub fn max_x(&self) -> f64 { self.x + self.w }
    pub fn max_y(&self) -> f64 { self.y + self.h }
    pub fn mid_x(&self) -> f64 { self.x + self.w / 2.0 }
    pub fn mid_y(&self) -> f64 { self.y + self.h / 2.0 }

    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.max_x() && py >= self.y && py < self.max_y()
    }

    /// Area of overlap (0 when disjoint).
    pub fn overlap_area(&self, o: &Rect) -> f64 {
        let w = self.max_x().min(o.max_x()) - self.x.max(o.x);
        let h = self.max_y().min(o.max_y()) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }

    pub fn inset(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w - 2.0 * dx, self.h - 2.0 * dy)
    }
}

/// One display, measured on the main thread.
#[derive(Clone, Debug)]
pub struct Display {
    pub key: String,
    pub name: String,
    /// Whole display, AX coordinates.
    pub full: Rect,
    /// Without menu bar and Dock, AX coordinates.
    pub usable: Rect,
    /// AppKit frames (for placing Reel's own panels).
    pub appkit_full: Rect,
    pub appkit_usable: Rect,
}

fn from_ns(r: NSRect) -> Rect {
    Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height)
}

pub fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0)
}

/// AppKit rect → AX rect.
pub fn to_ax(r: Rect, primary_height: f64) -> Rect {
    Rect::new(r.x, primary_height - r.max_y(), r.w, r.h)
}

/// AX rect → AppKit rect.
pub fn to_appkit(r: Rect, primary_height: f64) -> Rect {
    Rect::new(r.x, primary_height - r.max_y(), r.w, r.h)
}

pub fn displays(mtm: MainThreadMarker) -> Vec<Display> {
    let ph = primary_height(mtm);
    NSScreen::screens(mtm).iter().map(|s| describe(&s, ph)).collect()
}

fn describe(s: &Retained<NSScreen>, ph: f64) -> Display {
    let full = from_ns(s.frame());
    let usable = from_ns(s.visibleFrame());
    Display {
        key: display_key(s),
        name: s.localizedName().to_string(),
        full: to_ax(full, ph),
        usable: to_ax(usable, ph),
        appkit_full: full,
        appkit_usable: usable,
    }
}

/// A stable identifier for a display (its UUID), falling back to its name.
fn display_key(s: &NSScreen) -> String {
    let desc = s.deviceDescription();
    let key = NSString::from_str("NSScreenNumber");
    let number = desc.objectForKey(&key).and_then(|o| o.downcast::<NSNumber>().ok());
    if let Some(n) = number {
        unsafe {
            if let Some(uuid) = crate::ffi::Cf::owned(crate::ffi::CGDisplayCreateUUIDFromDisplayID(n.unsignedIntValue())) {
                if let Some(cf) = crate::ffi::Cf::owned(crate::ffi::CFUUIDCreateString(std::ptr::null(), uuid.0)) {
                    if let Some(s) = crate::ffi::string_from_cf(cf.0) {
                        return s;
                    }
                }
            }
        }
    }
    s.localizedName().to_string()
}
