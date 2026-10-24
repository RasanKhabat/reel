//! Keyboard shortcuts and the scroll gesture.
//!
//! Shortcuts use Carbon's RegisterEventHotKey (no extra permission). Combinations macOS
//! refuses there (some ⇧/⌥-only ones) fall back to a keyboard event tap. Both event taps
//! (that fallback and the scroll gesture) run on their own threads: a tap on the main
//! thread would make every keystroke or scroll on the Mac wait whenever Reel is busy.
//! The tap threads only decide "swallow or pass" and forward what happened to the main
//! thread.

use crate::ffi::*;
use crate::shortcuts::{Combo, Command, CMD, CONTROL, OPTION, SHIFT};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// What the input threads report to the main thread.
#[derive(Debug, Clone, Copy)]
pub enum Input {
    Shortcut(Command),
    /// A gesture started at this (AX) point.
    ScrollBegin { x: f64, y: f64 },
    /// Horizontal content delta in points (positive = fingers moved right).
    ScrollDelta(f64),
    /// Fingers lifted; velocity in points/s.
    ScrollEnd(f64),
    /// A notch of a mouse wheel (no begin/end phases).
    WheelDelta { x: f64, y: f64, delta: f64 },
    MinimizeSafari,
}

/// Where input events go (set once by the app).
static SINK: Mutex<Option<fn(Input)>> = Mutex::new(None);

pub fn set_sink(f: fn(Input)) {
    *SINK.lock().unwrap() = Some(f);
}

fn send(i: Input) {
    if let Some(f) = *SINK.lock().unwrap() {
        main_async_send(move || f(i));
    }
}

// MARK: Carbon hot keys (main thread)

thread_local! {
    static HOTKEYS: RefCell<HotKeyState> = RefCell::new(HotKeyState::default());
}

#[derive(Default)]
struct HotKeyState {
    handler_installed: bool,
    refs: Vec<*const c_void>,
    actions: HashMap<u32, Command>,
    next_id: u32,
}

const SIGNATURE: u32 = u32::from_be_bytes(*b"REEL");
const FIXED_SIGNATURE: u32 = u32::from_be_bytes(*b"REXT");

extern "C" fn hotkey_handler(_call: *const c_void, event: *const c_void, _user: *mut c_void) -> OSStatus {
    let mut hk = EventHotKeyID::default();
    unsafe {
        GetEventParameter(event, kEventParamDirectObject, typeEventHotKeyID, std::ptr::null_mut(),
                          std::mem::size_of::<EventHotKeyID>(), std::ptr::null_mut(), &mut hk as *mut _ as *mut c_void);
    }
    if hk.signature == FIXED_SIGNATURE {
        send(Input::MinimizeSafari);
        return 0;
    }
    let cmd = HOTKEYS.with(|h| h.borrow().actions.get(&hk.id).copied());
    if let Some(c) = cmd {
        send(Input::Shortcut(c));
    }
    0
}

fn ensure_handler(h: &mut HotKeyState) {
    if !h.handler_installed {
        let spec = EventTypeSpec { event_class: kEventClassKeyboard, event_kind: kEventHotKeyPressed };
        unsafe {
            InstallEventHandler(GetApplicationEventTarget(), hotkey_handler, 1, &spec, std::ptr::null_mut(), std::ptr::null_mut());
        }
        h.handler_installed = true;
    }
}

/// ⌘⇧↓, registered once for the whole run (not part of the keymap).
pub fn install_fixed() {
    HOTKEYS.with(|h| {
        let mut h = h.borrow_mut();
        ensure_handler(&mut h);
        let mut out: *const c_void = std::ptr::null();
        let status = unsafe {
            RegisterEventHotKey(crate::shortcuts::key::DOWN, CMD | SHIFT, EventHotKeyID { signature: FIXED_SIGNATURE, id: 1 },
                                GetApplicationEventTarget(), 0, &mut out)
        };
        if status != 0 {
            crate::log::write(&format!("fixed hotkey refused (status {status})"));
        }
    });
}

/// Registers the keymap. Returns the combos nobody could take (shown in the menu).
pub fn install(bindings: &HashMap<Command, Combo>) -> Vec<Combo> {
    uninstall();
    let mut fallback = HashMap::new();
    let mut failed = vec![];
    HOTKEYS.with(|h| {
        let mut h = h.borrow_mut();
        ensure_handler(&mut h);
        let mut list: Vec<(&Command, &Combo)> = bindings.iter().collect();
        list.sort_by_key(|(c, _)| **c);
        for (cmd, combo) in list {
            h.next_id += 1;
            let id = h.next_id;
            let mut out: *const c_void = std::ptr::null();
            let status = unsafe {
                RegisterEventHotKey(combo.key_code, combo.modifiers, EventHotKeyID { signature: SIGNATURE, id },
                                    GetApplicationEventTarget(), 0, &mut out)
            };
            if status == 0 && !out.is_null() {
                h.refs.push(out);
                h.actions.insert(id, *cmd);
            } else {
                crate::log::write(&format!("Carbon refused {} (status {status}); using event tap", combo.glyphs()));
                fallback.insert((combo.key_code, combo.modifiers), *cmd);
            }
        }
    });
    if !fallback.is_empty() {
        let combos: Vec<Combo> = fallback.keys().map(|(k, m)| Combo::new(*k, *m)).collect();
        *KEY_FALLBACK.lock().unwrap() = fallback;
        if !start_keyboard_tap() {
            KEY_FALLBACK.lock().unwrap().clear();
            failed = combos;
        }
    }
    failed
}

pub fn uninstall() {
    HOTKEYS.with(|h| {
        let mut h = h.borrow_mut();
        for r in h.refs.drain(..) {
            unsafe { UnregisterEventHotKey(r) };
        }
        h.actions.clear();
    });
    KEY_FALLBACK.lock().unwrap().clear();
}

// MARK: Keyboard fallback tap (own thread)

static KEY_FALLBACK: std::sync::LazyLock<Mutex<HashMap<(u32, u32), Command>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
static KEY_TAP_STARTED: AtomicBool = AtomicBool::new(false);
static KEY_TAP_OK: AtomicBool = AtomicBool::new(false);
static mut KEY_TAP: CFMachPortRef = std::ptr::null();

fn carbon_mods(flags: u64) -> u32 {
    let mut m = 0;
    if flags & kCGEventFlagMaskShift != 0 { m |= SHIFT; }
    if flags & kCGEventFlagMaskControl != 0 { m |= CONTROL; }
    if flags & kCGEventFlagMaskAlternate != 0 { m |= OPTION; }
    if flags & kCGEventFlagMaskCommand != 0 { m |= CMD; }
    m
}

extern "C" fn key_tap(_proxy: *const c_void, kind: u32, event: CGEventRef, _user: *mut c_void) -> CGEventRef {
    if kind == kCGEventTapDisabledByTimeout || kind == kCGEventTapDisabledByUserInput {
        unsafe { CGEventTapEnable(KEY_TAP, true) };
        return event;
    }
    if kind != kCGEventKeyDown {
        return event;
    }
    let code = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) } as u32;
    let mods = carbon_mods(unsafe { CGEventGetFlags(event) });
    let hit = KEY_FALLBACK.lock().map(|m| m.get(&(code, mods)).copied()).ok().flatten();
    match hit {
        Some(cmd) => {
            send(Input::Shortcut(cmd));
            std::ptr::null()
        }
        None => event,
    }
}

/// Starts the keyboard tap thread once. Returns whether the tap exists.
fn start_keyboard_tap() -> bool {
    if KEY_TAP_STARTED.swap(true, Ordering::SeqCst) {
        return KEY_TAP_OK.load(Ordering::SeqCst);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("reel-keys".into())
        .spawn(move || unsafe {
            let tap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
                                       1 << kCGEventKeyDown, key_tap, std::ptr::null_mut());
            if tap.is_null() {
                let _ = tx.send(false);
                return;
            }
            KEY_TAP = tap;
            let src = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            let _ = tx.send(true);
            CFRunLoopRun();
        })
        .expect("spawn key tap thread");
    let ok = rx.recv().unwrap_or(false);
    KEY_TAP_OK.store(ok, Ordering::SeqCst);
    if !ok {
        crate::log::write("could not create keyboard event tap");
    }
    ok
}

// MARK: Scroll gesture tap (own thread)

static SCROLL_ON: AtomicBool = AtomicBool::new(false);
/// The exact modifier flags that turn scrolling into strip scrolling.
static SCROLL_FLAGS: AtomicU64 = AtomicU64::new(kCGEventFlagMaskControl | kCGEventFlagMaskAlternate);
static SCROLL_TAP_STARTED: AtomicBool = AtomicBool::new(false);
static SCROLL_TAP_OK: AtomicBool = AtomicBool::new(false);
static mut SCROLL_TAP: CFMachPortRef = std::ptr::null();

pub fn set_scroll_enabled(on: bool) {
    SCROLL_ON.store(on, Ordering::SeqCst);
}

pub fn set_scroll_flags(flags: u64) {
    SCROLL_FLAGS.store(flags, Ordering::SeqCst);
}

struct Gesture {
    active: bool,
    samples: Vec<(Instant, f64)>,
}

thread_local! {
    static GESTURE: RefCell<Gesture> = const { RefCell::new(Gesture { active: false, samples: Vec::new() }) };
}

fn velocity(samples: &[(Instant, f64)]) -> f64 {
    let now = Instant::now();
    let recent: Vec<&(Instant, f64)> = samples.iter().filter(|(t, _)| now.duration_since(*t).as_secs_f64() < 0.1).collect();
    if recent.len() < 2 {
        return 0.0;
    }
    let span = now.duration_since(recent[0].0).as_secs_f64().max(1.0 / 120.0);
    recent.iter().map(|(_, d)| d).sum::<f64>() / span
}

extern "C" fn scroll_tap(_proxy: *const c_void, kind: u32, event: CGEventRef, _user: *mut c_void) -> CGEventRef {
    if kind == kCGEventTapDisabledByTimeout || kind == kCGEventTapDisabledByUserInput {
        unsafe { CGEventTapEnable(SCROLL_TAP, true) };
        return event;
    }
    if kind != kCGEventScrollWheel {
        return event;
    }
    let swallow = GESTURE.with(|g| {
        let mut g = g.borrow_mut();
        unsafe {
            let flags = CGEventGetFlags(event)
                & (kCGEventFlagMaskControl | kCGEventFlagMaskAlternate | kCGEventFlagMaskShift | kCGEventFlagMaskCommand);
            let held = SCROLL_ON.load(Ordering::Relaxed) && flags == SCROLL_FLAGS.load(Ordering::Relaxed);
            let continuous = CGEventGetIntegerValueField(event, kCGScrollWheelEventIsContinuous) != 0;
            let phase = CGEventGetIntegerValueField(event, kCGScrollWheelEventScrollPhase);
            let momentum = CGEventGetIntegerValueField(event, kCGScrollWheelEventMomentumPhase);
            let dx = CGEventGetDoubleValueField(event, kCGScrollWheelEventPointDeltaAxis2);
            let dy = CGEventGetDoubleValueField(event, kCGScrollWheelEventPointDeltaAxis1);

            if !continuous {
                // Mouse wheel: no phases; the main thread ends the drag after a pause.
                if !held {
                    return false;
                }
                let p = CGEventGetLocation(event);
                send(Input::WheelDelta { x: p.x, y: p.y, delta: if dx != 0.0 { dx } else { dy * 4.0 } });
                return true;
            }
            // Trackpad phases: 1 began, 2 changed, 4 ended, 8 cancelled, 128 may-begin.
            if g.active {
                if momentum != 0 {
                    return true; // Reel does its own coasting
                }
                if phase == 4 || phase == 8 {
                    g.active = false;
                    send(Input::ScrollEnd(velocity(&g.samples)));
                    g.samples.clear();
                    return true;
                }
                if dx != 0.0 {
                    let now = Instant::now();
                    g.samples.push((now, dx));
                    g.samples.retain(|(t, _)| now.duration_since(*t).as_secs_f64() < 0.1);
                    send(Input::ScrollDelta(dx));
                }
                return true;
            }
            if !(held && (phase == 1 || phase == 128 || (phase == 2 && dx.abs() > dy.abs()))) {
                return false;
            }
            g.active = true;
            g.samples.clear();
            let p = CGEventGetLocation(event);
            send(Input::ScrollBegin { x: p.x, y: p.y });
            if phase == 2 && dx != 0.0 {
                g.samples.push((Instant::now(), dx));
                send(Input::ScrollDelta(dx));
            }
            true
        }
    });
    if swallow { std::ptr::null() } else { event }
}

/// Starts the scroll tap thread once. Returns whether swiping is available.
pub fn start_scroll_tap() -> bool {
    if SCROLL_TAP_STARTED.swap(true, Ordering::SeqCst) {
        return SCROLL_TAP_OK.load(Ordering::SeqCst);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("reel-scroll".into())
        .spawn(move || unsafe {
            let tap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
                                       1 << kCGEventScrollWheel, scroll_tap, std::ptr::null_mut());
            if tap.is_null() {
                let _ = tx.send(false);
                return;
            }
            SCROLL_TAP = tap;
            let src = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            let _ = tx.send(true);
            CFRunLoopRun();
        })
        .expect("spawn scroll tap thread");
    let ok = rx.recv().unwrap_or(false);
    SCROLL_TAP_OK.store(ok, Ordering::SeqCst);
    if !ok {
        crate::log::write("could not create scroll event tap");
    }
    ok
}
