//! Window and app events: one AXObserver per app (window created, focused/main window
//! changed; per managed window: destroyed, minimized, restored, resized) plus NSWorkspace
//! app launch / quit / activation. Everything is delivered on the main thread.

use crate::ax::{self, Element};
use crate::ffi::*;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSOperationQueue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

pub enum WindowEvent {
    Created(Element, i32),
    Destroyed(Element),
    Minimized(Element),
    Restored(Element, i32),
    Resized(Element),
    /// An app came to the front or changed its focused window.
    FocusChanged { pid: i32, window: Option<Element> },
    AppQuit(i32),
}

const CREATED: &str = "AXWindowCreated";
const FOCUSED_CHANGED: &str = "AXFocusedWindowChanged";
const MAIN_CHANGED: &str = "AXMainWindowChanged";
const DESTROYED: &str = "AXUIElementDestroyed";
const MINIATURIZED: &str = "AXWindowMiniaturized";
const DEMINIATURIZED: &str = "AXWindowDeminiaturized";
const RESIZED: &str = "AXWindowResized";

#[derive(Default)]
struct State {
    sink: Option<fn(WindowEvent)>,
    observers: HashMap<i32, AXObserverRef>,
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn emit(e: WindowEvent) {
    if let Some(sink) = STATE.with(|s| s.borrow().sink) {
        sink(e);
    }
}

extern "C" fn callback(_obs: AXObserverRef, el: AXUIElementRef, name: CFStringRef, _refcon: *mut c_void) {
    let Some(name) = string_from_cf(name) else { return };
    let Some(el) = Element::retained(el) else { return };
    let pid = el.pid();
    match name.as_str() {
        CREATED => emit(WindowEvent::Created(el, pid)),
        DESTROYED => emit(WindowEvent::Destroyed(el)),
        MINIATURIZED => emit(WindowEvent::Minimized(el)),
        DEMINIATURIZED => emit(WindowEvent::Restored(el, pid)),
        RESIZED => emit(WindowEvent::Resized(el)),
        FOCUSED_CHANGED | MAIN_CHANGED => {
            let window = (el.role().as_deref() == Some(ax::WINDOW_ROLE)).then_some(el);
            emit(WindowEvent::FocusChanged { pid, window })
        }
        _ => {}
    }
}

fn observe(pid: i32) -> Option<AXObserverRef> {
    if let Some(o) = STATE.with(|s| s.borrow().observers.get(&pid).copied()) {
        return Some(o);
    }
    let mut obs: AXObserverRef = std::ptr::null();
    unsafe {
        if AXObserverCreate(pid, callback, &mut obs) != kAXErrorSuccess || obs.is_null() {
            return None;
        }
        let app = Element::application(pid);
        for n in [CREATED, FOCUSED_CHANGED, MAIN_CHANGED] {
            let name = cfstr(n);
            AXObserverAddNotification(obs, app.raw(), name.0, std::ptr::null_mut());
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(obs), kCFRunLoopCommonModes);
    }
    STATE.with(|s| s.borrow_mut().observers.insert(pid, obs));
    Some(obs)
}

fn forget(pid: i32) {
    if let Some(obs) = STATE.with(|s| s.borrow_mut().observers.remove(&pid)) {
        unsafe {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(obs), kCFRunLoopCommonModes);
            CFRelease(obs);
        }
    }
}

/// Subscribes to per-window notifications for a window Reel manages.
pub fn watch(window: &Element, pid: i32) {
    let Some(obs) = observe(pid) else { return };
    for n in [DESTROYED, MINIATURIZED, DEMINIATURIZED, RESIZED] {
        let name = cfstr(n);
        unsafe { AXObserverAddNotification(obs, window.raw(), name.0, std::ptr::null_mut()) };
    }
}

fn app_from(note: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    let info = note.userInfo()?;
    let obj = info.objectForKey(unsafe { NSWorkspaceApplicationKey })?;
    obj.downcast::<NSRunningApplication>().ok()
}

pub fn start(sink: fn(WindowEvent)) {
    STATE.with(|s| s.borrow_mut().sink = Some(sink));
    for app in crate::apps::managed() {
        observe(app.processIdentifier());
    }
    let ws = NSWorkspace::sharedWorkspace();
    let center = ws.notificationCenter();
    let queue = NSOperationQueue::mainQueue();
    let launched = RcBlock::new(|note: NonNull<NSNotification>| {
        if let Some(app) = app_from(unsafe { note.as_ref() }) {
            if crate::apps::manages(&app) {
                observe(app.processIdentifier());
            }
        }
    });
    let terminated = RcBlock::new(|note: NonNull<NSNotification>| {
        if let Some(app) = app_from(unsafe { note.as_ref() }) {
            let pid = app.processIdentifier();
            forget(pid);
            emit(WindowEvent::AppQuit(pid));
        }
    });
    let activated = RcBlock::new(|note: NonNull<NSNotification>| {
        if let Some(app) = app_from(unsafe { note.as_ref() }) {
            emit(WindowEvent::FocusChanged { pid: app.processIdentifier(), window: None });
        }
    });
    let tokens = unsafe {
        vec![
            center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidLaunchApplicationNotification), None, Some(&queue), &launched),
            center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidTerminateApplicationNotification), None, Some(&queue), &terminated),
            center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidActivateApplicationNotification), None, Some(&queue), &activated),
        ]
    };
    STATE.with(|s| s.borrow_mut().tokens = tokens);
}

pub fn stop() {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let (tokens, pids) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.sink = None;
        (std::mem::take(&mut s.tokens), s.observers.keys().copied().collect::<Vec<_>>())
    });
    for t in tokens {
        unsafe { center.removeObserver(t.as_ref()) };
    }
    for pid in pids {
        forget(pid);
    }
}
