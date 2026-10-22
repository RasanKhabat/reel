//! The few Objective-C classes Reel defines, each just forwarding to a Rust closure.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindowDelegate};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSPoint};
use std::cell::RefCell;

pub type Callback = Box<dyn Fn(&AnyObject)>;

pub struct TargetIvars {
    f: Callback,
}

define_class!(
    /// Target for buttons, menu items, timers and display links: `fire:` calls the closure.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    pub struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        #[unsafe(method(fire:))]
        fn fire(&self, sender: &AnyObject) {
            (self.ivars().f)(sender);
        }
    }
);

impl Target {
    pub fn new(mtm: MainThreadMarker, f: impl Fn(&AnyObject) + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { f: Box::new(f) });
        unsafe { msg_send![super(this), init] }
    }

    pub fn action() -> objc2::runtime::Sel {
        sel!(fire:)
    }
}

pub struct WindowDelegateIvars {
    will_close: Box<dyn Fn()>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowDelegateIvars]
    pub struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _n: &NSNotification) {
            (self.ivars().will_close)();
        }
    }
);

impl WindowDelegate {
    pub fn new(mtm: MainThreadMarker, will_close: impl Fn() + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WindowDelegateIvars { will_close: Box::new(will_close) });
        unsafe { msg_send![super(this), init] }
    }
}

pub struct HudViewIvars {
    on_click: Box<dyn Fn(NSPoint)>,
    on_hover: Box<dyn Fn(bool)>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    /// The overview pill's view: reports clicks (in view coordinates) and hover.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = HudViewIvars]
    pub struct HudView;

    unsafe impl NSObjectProtocol for HudView {}

    impl HudView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _e: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            (self.ivars().on_click)(p);
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _e: &NSEvent) {
            (self.ivars().on_hover)(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _e: &NSEvent) {
            (self.ivars().on_hover)(false);
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            unsafe {
                let _: () = msg_send![super(self), updateTrackingAreas];
                if let Some(old) = self.ivars().tracking.borrow_mut().take() {
                    self.removeTrackingArea(&old);
                }
                let area = NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::ActiveAlways
                        | NSTrackingAreaOptions::InVisibleRect,
                    Some(self),
                    None,
                );
                self.addTrackingArea(&area);
                *self.ivars().tracking.borrow_mut() = Some(area);
            }
        }
    }
);

impl HudView {
    pub fn new(mtm: MainThreadMarker, on_click: impl Fn(NSPoint) + 'static, on_hover: impl Fn(bool) + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HudViewIvars {
            on_click: Box::new(on_click),
            on_hover: Box::new(on_hover),
            tracking: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }
}
