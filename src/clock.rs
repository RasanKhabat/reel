//! Calls the desk once per display refresh while anything is moving. Uses the main
//! screen's CADisplayLink on macOS 14+ (in step with ProMotion), a 120 Hz timer before
//! that. Stops itself as soon as everything has settled.

use crate::objc_classes::Target;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes, NSTimer};
use objc2_quartz_core::CADisplayLink;
use std::cell::RefCell;

enum Driver {
    Link(Retained<CADisplayLink>),
    Timer(Retained<NSTimer>),
}

struct Clock {
    _target: Retained<Target>,
    driver: Driver,
}

thread_local! {
    static CLOCK: RefCell<Option<Clock>> = const { RefCell::new(None) };
}

pub fn ensure(mtm: MainThreadMarker) {
    if CLOCK.with(|c| c.borrow().is_some()) {
        return;
    }
    crate::with_desk(|d| d.last_tick = std::time::Instant::now());
    let target = Target::new(mtm, |_: &AnyObject| {
        crate::with_desk(|d| {
            if !d.tick() {
                crate::ffi::main_async(stop);
            }
        })
    });
    let screen = NSScreen::mainScreen(mtm);
    let driver = match screen {
        Some(s) if s.respondsToSelector(sel!(displayLinkWithTarget:selector:)) => unsafe {
            let link = s.displayLinkWithTarget_selector(&target, Target::action());
            link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes);
            Driver::Link(link)
        },
        _ => unsafe {
            let t = NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(1.0 / 120.0, &target, Target::action(), None, true);
            NSRunLoop::mainRunLoop().addTimer_forMode(&t, NSRunLoopCommonModes);
            Driver::Timer(t)
        },
    };
    CLOCK.with(|c| *c.borrow_mut() = Some(Clock { _target: target, driver }));
}

pub fn stop() {
    // Something may have started moving again since this stop was queued.
    let still_moving = crate::with_desk_sync(|d| d.any_animating()).unwrap_or(false);
    if still_moving {
        return;
    }
    if let Some(c) = CLOCK.with(|c| c.borrow_mut().take()) {
        match c.driver {
            Driver::Link(l) => l.invalidate(),
            Driver::Timer(t) => t.invalidate(),
        }
    }
}
