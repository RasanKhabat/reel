//! Reel: a scrolling window manager for macOS.
//!
//! Windows sit side by side on a horizontal strip wider than the screen; moving focus
//! slides the strip so the focused window is always fully visible.

mod apps;
mod ax;
mod clock;
mod controller;
mod desk;
mod editor;
mod events;
mod ffi;
mod geometry;
mod input;
mod layout_store;
mod log;
mod login_item;
mod menu;
mod mover;
mod objc_classes;
mod overlays;
mod prefs;
mod shortcuts;
mod spring;
mod strip;
mod test_mode;
mod testctl;
mod watchdog;

use block2::RcBlock;
use desk::Desk;
use mover::Mover;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDidChangeScreenParametersNotification,
    NSApplicationWillTerminateNotification, NSEvent, NSWorkspace,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue, NSString, NSURL};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// ~/Library/Application Support/Reel (or the test directory).
pub fn support_dir() -> PathBuf {
    test_mode::directory().cloned().unwrap_or_else(|| home().join("Library/Application Support/Reel"))
}

// MARK: Main-thread state

thread_local! {
    static DESK: RefCell<Option<Desk>> = const { RefCell::new(None) };
    static MOVER: RefCell<Mover> = RefCell::new(Mover::default());
    static SCREEN_GENERATION: Cell<u64> = const { Cell::new(0) };
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("main thread")
}

/// Runs `f` with the desk. If the desk is already in use further up the stack (a
/// callback fired during a desk method), `f` runs right after instead.
pub fn with_desk(f: impl FnOnce(&mut Desk) + 'static) {
    DESK.with(|d| match d.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(desk) = guard.as_mut() {
                f(desk)
            }
        }
        Err(_) => ffi::main_async(move || with_desk(f)),
    })
}

/// Reads from the desk now, or None if it's busy.
pub fn with_desk_sync<R>(f: impl FnOnce(&mut Desk) -> R) -> Option<R> {
    DESK.with(|d| d.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub fn with_mover<R>(f: impl FnOnce(&mut Mover) -> R) -> R {
    MOVER.with(|m| f(&mut m.borrow_mut()))
}

/// Runs `f` with the desk after `seconds`.
pub fn later(seconds: f64, f: impl FnOnce(&mut Desk) + 'static) {
    ffi::main_after(seconds, move || with_desk(f));
}

pub fn ensure_clock() {
    clock::ensure(mtm());
}

pub fn close_menu() {
    menu::close();
}

/// The pointer position in AX (top-left origin) coordinates.
pub fn mouse_location_ax(mtm: MainThreadMarker) -> (f64, f64) {
    let p = NSEvent::mouseLocation();
    (p.x, geometry::primary_height(mtm) - p.y)
}

pub fn set_tiling(on: bool) {
    with_desk(move |d| {
        if on {
            d.start_tiling();
        } else {
            d.stop_tiling(false);
        }
        ensure_clock();
    });
}

/// Orderly exit: put windows back, tell the watchdog, leave.
pub fn quit() {
    DESK.with(|d| {
        if let Ok(mut g) = d.try_borrow_mut() {
            if let Some(desk) = g.as_mut() {
                desk.stop_tiling(!test_mode::is_on());
            }
        }
    });
    watchdog::mark_clean_exit();
    log::write("quit");
    std::process::exit(0);
}

fn open_url(s: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(s)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

pub fn open_accessibility_settings() {
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
}

pub fn open_log() {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&log::path().display().to_string()));
    NSWorkspace::sharedWorkspace().openURL(&url);
}

// MARK: Lifecycle

static SIGNALLED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    SIGNALLED.store(true, Ordering::SeqCst);
}

/// Quit from Activity Monitor / `kill` / logout arrives as a signal; tidy up the same way.
/// (A force quit, SIGKILL, can't be caught; the watchdog covers that.)
fn watch_signals() {
    unsafe {
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(sig, on_signal as *const () as libc::sighandler_t);
        }
    }
    fn poll() {
        if SIGNALLED.load(Ordering::SeqCst) {
            log::write("signal: restoring windows and quitting");
            quit();
        }
        ffi::main_after(0.25, poll);
    }
    poll();
}

fn wait_for_trust() {
    if ax::is_trusted() {
        did_become_trusted();
    } else {
        ffi::main_after(1.0, wait_for_trust);
    }
}

fn did_become_trusted() {
    log::write("accessibility granted");
    let (was_on, ..) = with_desk_sync(|d| (d.tiling_was_on(),)).unwrap_or((false,));
    if test_mode::is_on() {
        // Tests drive tiling themselves; after a watchdog relaunch, resume it.
        if watchdog::is_recovery_launch() && was_on {
            set_tiling(true);
        }
        testctl::start();
        return;
    }
    if was_on {
        set_tiling(true);
    } else {
        // Windows may still be parked off screen if Reel was force-quit while tiling.
        with_desk(|d| {
            d.rescue_offscreen_windows();
        });
    }
}

fn observe_app_notifications() {
    let center = NSNotificationCenter::defaultCenter();
    let queue = NSOperationQueue::mainQueue();
    let screens = RcBlock::new(|_: NonNull<NSNotification>| {
        // Displays arrive and leave in bursts of notifications; act once things settle.
        let generation = SCREEN_GENERATION.with(|g| {
            g.set(g.get() + 1);
            g.get()
        });
        later(0.6, move |d| {
            if SCREEN_GENERATION.with(|g| g.get()) == generation {
                d.screens_changed();
                ensure_clock();
            }
        });
    });
    let terminate = RcBlock::new(|_: NonNull<NSNotification>| quit());
    unsafe {
        let a = center.addObserverForName_object_queue_usingBlock(Some(NSApplicationDidChangeScreenParametersNotification), None, Some(&queue), &screens);
        let b = center.addObserverForName_object_queue_usingBlock(Some(NSApplicationWillTerminateNotification), None, Some(&queue), &terminate);
        // Observers live for the whole run.
        std::mem::forget(a);
        std::mem::forget(b);
    }
}

fn main() {
    let mtm = mtm();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    watchdog::start();
    DESK.with(|d| *d.borrow_mut() = Some(Desk::new(mtm)));
    input::set_sink(|i| with_desk(move |d| d.input(i)));
    if !test_mode::is_on() {
        menu::install(mtm);
    }
    let trusted = ax::is_trusted();
    log::write(&format!(
        "launched{} (Rust); accessibility {}",
        if watchdog::is_recovery_launch() { " after a force quit" } else { "" },
        if trusted { "granted" } else { "NOT granted (waiting)" }
    ));
    if !trusted {
        ax::request_trust();
    }
    watch_signals();
    observe_app_notifications();
    ffi::main_async(wait_for_trust);
    app.run();
}
