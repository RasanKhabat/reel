//! Raw bindings to the C APIs Reel uses: Core Foundation basics, Accessibility,
//! Carbon hot keys, Quartz event taps, display UUIDs and Grand Central Dispatch.
#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use std::ffi::c_void;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFRunLoopRef = *const c_void;
pub type CFRunLoopSourceRef = *const c_void;
pub type CFMachPortRef = *const c_void;
pub type CFIndex = isize;
pub type CFTypeID = usize;
pub type AXUIElementRef = *const c_void;
pub type AXObserverRef = *const c_void;
pub type AXValueRef = *const c_void;
pub type AXError = i32;
pub type CGEventRef = *const c_void;
pub type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

pub const kAXErrorSuccess: AXError = 0;
pub const kAXErrorInvalidUIElement: AXError = -25202;
pub const kAXValueCGPointType: u32 = 1;
pub const kAXValueCGSizeType: u32 = 2;
pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> u8;
    pub fn CFHash(cf: CFTypeRef) -> usize;
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFBooleanGetTypeID() -> CFTypeID;
    pub fn CFArrayGetTypeID() -> CFTypeID;
    pub fn CFBooleanGetValue(b: CFTypeRef) -> u8;
    pub fn CFStringCreateWithBytes(alloc: CFTypeRef, bytes: *const u8, len: CFIndex, enc: u32, external: u8) -> CFStringRef;
    pub fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    pub fn CFStringGetMaximumSizeForEncoding(len: CFIndex, enc: u32) -> CFIndex;
    pub fn CFStringGetCString(s: CFStringRef, buf: *mut u8, size: CFIndex, enc: u32) -> u8;
    pub fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(a: CFArrayRef, i: CFIndex) -> CFTypeRef;
    pub fn CFDictionaryCreate(alloc: CFTypeRef, keys: *const CFTypeRef, values: *const CFTypeRef, n: CFIndex,
                              key_cb: *const c_void, value_cb: *const c_void) -> CFDictionaryRef;
    pub fn CFRunLoopGetMain() -> CFRunLoopRef;
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, src: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRemoveSource(rl: CFRunLoopRef, src: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRun();
    pub fn CFMachPortCreateRunLoopSource(alloc: CFTypeRef, port: CFMachPortRef, order: CFIndex) -> CFRunLoopSourceRef;
    pub fn CFMachPortInvalidate(port: CFMachPortRef);
    pub fn CFUUIDCreateString(alloc: CFTypeRef, uuid: CFTypeRef) -> CFStringRef;
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFBooleanFalse: CFTypeRef;
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> u8;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> AXError;
    pub fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> AXError;
    pub fn AXUIElementPerformAction(el: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXUIElementGetPid(el: AXUIElementRef, pid: *mut i32) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> AXError;
    pub fn AXValueCreate(kind: u32, value: *const c_void) -> AXValueRef;
    pub fn AXValueGetValue(v: AXValueRef, kind: u32, out: *mut c_void) -> u8;
    pub fn AXValueGetTypeID() -> CFTypeID;
    pub fn AXObserverCreate(pid: i32, callback: AXObserverCallback, out: *mut AXObserverRef) -> AXError;
    pub fn AXObserverAddNotification(obs: AXObserverRef, el: AXUIElementRef, name: CFStringRef, refcon: *mut c_void) -> AXError;
    pub fn AXObserverGetRunLoopSource(obs: AXObserverRef) -> CFRunLoopSourceRef;
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;

    // Quartz event taps / events.
    pub fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: CGEventTapCallBack,
                            user: *mut c_void) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventGetIntegerValueField(e: CGEventRef, field: u32) -> i64;
    pub fn CGEventGetDoubleValueField(e: CGEventRef, field: u32) -> f64;
    pub fn CGEventGetFlags(e: CGEventRef) -> u64;
    pub fn CGEventGetLocation(e: CGEventRef) -> CGPoint;
    pub fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> CFTypeRef;
}

pub type AXObserverCallback = extern "C" fn(obs: AXObserverRef, el: AXUIElementRef, name: CFStringRef, refcon: *mut c_void);
pub type CGEventTapCallBack = extern "C" fn(proxy: *const c_void, kind: u32, event: CGEventRef, user: *mut c_void) -> CGEventRef;

// Event tap constants.
pub const kCGSessionEventTap: u32 = 1;
pub const kCGHeadInsertEventTap: u32 = 0;
pub const kCGEventTapOptionDefault: u32 = 0;
pub const kCGEventKeyDown: u32 = 10;
pub const kCGEventScrollWheel: u32 = 22;
pub const kCGEventTapDisabledByTimeout: u32 = 0xFFFF_FFFE;
pub const kCGEventTapDisabledByUserInput: u32 = 0xFFFF_FFFF;
pub const kCGKeyboardEventKeycode: u32 = 9;
pub const kCGScrollWheelEventIsContinuous: u32 = 88;
pub const kCGScrollWheelEventPointDeltaAxis1: u32 = 96;
pub const kCGScrollWheelEventPointDeltaAxis2: u32 = 97;
pub const kCGScrollWheelEventScrollPhase: u32 = 99;
pub const kCGScrollWheelEventMomentumPhase: u32 = 123;
pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;

// Carbon hot keys.
#[repr(C)]
pub struct EventTypeSpec {
    pub event_class: u32,
    pub event_kind: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct EventHotKeyID {
    pub signature: u32,
    pub id: u32,
}

pub type EventHandlerUPP = extern "C" fn(call: *const c_void, event: *const c_void, user: *mut c_void) -> OSStatus;
pub const kEventClassKeyboard: u32 = u32::from_be_bytes(*b"keyb");
pub const kEventHotKeyPressed: u32 = 5;
pub const kEventParamDirectObject: u32 = u32::from_be_bytes(*b"----");
pub const typeEventHotKeyID: u32 = u32::from_be_bytes(*b"hkid");

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub fn GetApplicationEventTarget() -> *const c_void;
    pub fn InstallEventHandler(target: *const c_void, handler: EventHandlerUPP, n: u32, list: *const EventTypeSpec,
                               user: *mut c_void, out: *mut *const c_void) -> OSStatus;
    pub fn RegisterEventHotKey(code: u32, mods: u32, id: EventHotKeyID, target: *const c_void, options: u32,
                               out: *mut *const c_void) -> OSStatus;
    pub fn UnregisterEventHotKey(hk: *const c_void) -> OSStatus;
    pub fn GetEventParameter(event: *const c_void, name: u32, kind: u32, out_kind: *mut u32, size: usize,
                             out_size: *mut usize, data: *mut c_void) -> OSStatus;
}

// Grand Central Dispatch: run closures on the main thread.
#[repr(C)]
pub struct DispatchQueue {
    _private: [u8; 0],
}

extern "C" {
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_async_f(q: *const DispatchQueue, ctx: *mut c_void, work: extern "C" fn(*mut c_void));
    fn dispatch_after_f(when: u64, q: *const DispatchQueue, ctx: *mut c_void, work: extern "C" fn(*mut c_void));
    fn dispatch_time(when: u64, delta: i64) -> u64;
}

extern "C" fn trampoline(ctx: *mut c_void) {
    let f: Box<Box<dyn FnOnce()>> = unsafe { Box::from_raw(ctx as *mut Box<dyn FnOnce()>) };
    f();
}

/// Runs `f` on the main thread, soon.
pub fn main_async(f: impl FnOnce() + 'static) {
    let b: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    unsafe { dispatch_async_f(&_dispatch_main_q, Box::into_raw(b) as *mut c_void, trampoline) };
}

/// Like `main_async` but callable from any thread (the closure must be Send).
pub fn main_async_send(f: impl FnOnce() + Send + 'static) {
    main_async(f)
}

/// Runs `f` on the main thread after `seconds`.
pub fn main_after(seconds: f64, f: impl FnOnce() + 'static) {
    let b: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    unsafe {
        let when = dispatch_time(0, (seconds * 1e9) as i64);
        dispatch_after_f(when, &_dispatch_main_q, Box::into_raw(b) as *mut c_void, trampoline);
    }
}

// MARK: Small Core Foundation helpers

/// An owned CF reference (released on drop).
pub struct Cf(pub CFTypeRef);

impl Cf {
    /// Takes ownership of a +1 reference (Create/Copy rule). None if null.
    pub fn owned(r: CFTypeRef) -> Option<Cf> {
        if r.is_null() { None } else { Some(Cf(r)) }
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

pub fn cfstr(s: &str) -> Cf {
    Cf(unsafe { CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as CFIndex, kCFStringEncodingUTF8, 0) })
}

/// Reads a CFString into a Rust String (borrowed reference).
pub fn string_from_cf(s: CFStringRef) -> Option<String> {
    if s.is_null() || unsafe { CFGetTypeID(s) != CFStringGetTypeID() } {
        return None;
    }
    unsafe {
        let len = CFStringGetLength(s);
        let cap = CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) + 1;
        let mut buf = vec![0u8; cap as usize];
        if CFStringGetCString(s, buf.as_mut_ptr(), cap, kCFStringEncodingUTF8) == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// The login session's properties (e.g. whether the screen is locked).
    pub fn CGSessionCopyCurrentDictionary() -> core_foundation::dictionary::CFDictionaryRef;
}
