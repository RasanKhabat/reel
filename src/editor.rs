//! The "Reel Shortcuts" window: click an action, press the new keys.
//!
//! While recording, Reel's own global shortcuts are switched off so the keys reach this
//! window instead of being acted on. While the window is open Reel behaves like a normal
//! app (Dock icon, ⌘Tab) so the window can always be found again; it can't go full
//! screen (that's what lost the Swift version's window behind everything).

use crate::objc_classes::{Target, WindowDelegate};
use crate::shortcuts::{key, Combo, Command, Preset, CMD, CONTROL, MODIFIER_LEGEND, OPTION, SHIFT};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSButton, NSEvent, NSEventMask,
    NSEventModifierFlags, NSFont, NSLayoutAttribute, NSScrollView, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::NonNull;

struct Editor {
    window: Retained<NSWindow>,
    _delegate: Retained<WindowDelegate>,
    message: Retained<NSTextField>,
    buttons: HashMap<Command, Retained<NSButton>>,
    /// Buttons hold their targets weakly; this keeps them alive.
    _targets: Vec<Retained<Target>>,
    recording: Option<Command>,
    monitor: Option<Retained<AnyObject>>,
}

thread_local! {
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("main thread")
}

fn label(text: &str, size: f64, bold: bool) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&NSString::from_str(text), mtm());
    let font = if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    l.setFont(Some(&font));
    l
}

fn button(title: &str, target: &Target) -> Retained<NSButton> {
    unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(Target::action()), mtm()) }
}

fn stack(views: &[Retained<NSView>], vertical: bool, spacing: f64) -> Retained<NSStackView> {
    let refs: Vec<&NSView> = views.iter().map(|v| &**v).collect();
    let s = NSStackView::stackViewWithViews(&NSArray::from_slice(&refs), mtm());
    s.setOrientation(if vertical { NSUserInterfaceLayoutOrientation::Vertical } else { NSUserInterfaceLayoutOrientation::Horizontal });
    s.setSpacing(spacing);
    if vertical {
        s.setAlignment(NSLayoutAttribute::Leading);
    }
    s
}

fn bindings() -> HashMap<Command, Combo> {
    crate::with_desk_sync(|d| d.bindings()).unwrap_or_default()
}

pub fn show() {
    let mtm = mtm();
    let app = NSApplication::sharedApplication(mtm);
    if EDITOR.with(|e| e.borrow().is_none()) {
        build(mtm);
    }
    refresh();
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    EDITOR.with(|e| {
        if let Some(ed) = e.borrow().as_ref() {
            ed.window.makeKeyAndOrderFront(None);
        }
    });
}

fn build(mtm: MainThreadMarker) {
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(600.0, 660.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable | NSWindowStyleMask::Resizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("Reel Shortcuts"));
    window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenNone | NSWindowCollectionBehavior::MoveToActiveSpace);

    let mut targets = vec![];
    let mut buttons = HashMap::new();
    let mut rows: Vec<Retained<NSView>> = vec![
        Retained::into_super(Retained::into_super(label("Click a shortcut, then press the keys you want.", 14.0, true))),
        Retained::into_super(Retained::into_super(label(MODIFIER_LEGEND, 11.0, false))),
    ];
    let message = label("", 12.0, false);
    rows.push(Retained::into_super(Retained::into_super(message.clone())));

    for (group, commands) in Command::groups() {
        rows.push(Retained::into_super(Retained::into_super(label(group, 12.0, true))));
        for c in commands {
            let t = Target::new(mtm, move |_: &AnyObject| crate::ffi::main_async(move || toggle_recording(c)));
            let b = button("", &t);
            b.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(11.0, 0.0)));
            b.setFrameSize(NSSize::new(300.0, 24.0));
            let title = label(c.title(), 12.0, false);
            title.setFrameSize(NSSize::new(250.0, 20.0));
            let row = stack(&[Retained::into_super(Retained::into_super(title)), Retained::into_super(Retained::into_super(b.clone()))], false, 12.0);
            rows.push(Retained::into_super(row));
            buttons.insert(c, b);
            targets.push(t);
        }
    }

    let reset_default = Target::new(mtm, |_: &AnyObject| crate::ffi::main_async(|| reset(Preset::Shift)));
    let resets = stack(&[Retained::into_super(Retained::into_super(button("Reset to the default shortcuts", &reset_default)))], false, 8.0);
    rows.push(Retained::into_super(resets));
    targets.push(reset_default);

    let content = stack(&rows, true, 6.0);
    content.setEdgeInsets(NSEdgeInsets { top: 16.0, left: 18.0, bottom: 16.0, right: 18.0 });
    let size = content.fittingSize();
    content.setFrameSize(size);
    let scroll = NSScrollView::new(mtm);
    scroll.setHasVerticalScroller(true);
    scroll.setDocumentView(Some(&content));
    window.setContentView(Some(&scroll));
    window.setContentSize(NSSize::new(size.width.max(560.0), 640.0));
    // Start scrolled to the top.
    content.scrollPoint(NSPoint::new(0.0, size.height));
    window.center();

    let delegate = WindowDelegate::new(mtm, || crate::ffi::main_async(closed));
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    EDITOR.with(|e| {
        *e.borrow_mut() = Some(Editor { window, _delegate: delegate, message, buttons, _targets: targets, recording: None, monitor: None })
    });
}

fn refresh() {
    let b = bindings();
    EDITOR.with(|e| {
        let e = e.borrow();
        let Some(ed) = e.as_ref() else { return };
        for (c, btn) in &ed.buttons {
            let text = if ed.recording == Some(*c) {
                "Press keys…".to_string()
            } else {
                b.get(c).map(|k| k.spelled()).unwrap_or_else(|| "— none —".into())
            };
            btn.setTitle(&NSString::from_str(&text));
        }
    });
}

fn say(text: &str) {
    EDITOR.with(|e| {
        if let Some(ed) = e.borrow().as_ref() {
            ed.message.setStringValue(&NSString::from_str(text));
        }
    });
}

fn toggle_recording(c: Command) {
    let current = EDITOR.with(|e| e.borrow().as_ref().and_then(|ed| ed.recording));
    stop_recording();
    if current == Some(c) {
        refresh();
        return;
    }
    crate::with_desk(|d| d.set_hotkeys_suspended(true));
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        let swallow = handle(unsafe { event.as_ref() });
        if swallow { std::ptr::null_mut() } else { event.as_ptr() }
    });
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    EDITOR.with(|e| {
        if let Some(ed) = e.borrow_mut().as_mut() {
            ed.recording = Some(c);
            ed.monitor = monitor;
        }
    });
    say(&format!("Press the new shortcut for “{}”. Esc cancels, ⌫ Delete removes it.", c.title()));
    refresh();
}

fn stop_recording() {
    let (monitor, was) = EDITOR.with(|e| match e.borrow_mut().as_mut() {
        Some(ed) => (ed.monitor.take(), ed.recording.take()),
        None => (None, None),
    });
    if let Some(m) = monitor {
        unsafe { NSEvent::removeMonitor(&m) };
    }
    if was.is_some() {
        crate::with_desk(|d| d.set_hotkeys_suspended(false));
    }
}

/// Returns true to swallow the key.
fn handle(event: &NSEvent) -> bool {
    let Some(c) = EDITOR.with(|e| e.borrow().as_ref().and_then(|ed| ed.recording)) else { return false };
    let flags = event.modifierFlags();
    let code = event.keyCode() as u32;
    let mut mods = 0;
    if flags.contains(NSEventModifierFlags::Control) { mods |= CONTROL; }
    if flags.contains(NSEventModifierFlags::Option) { mods |= OPTION; }
    if flags.contains(NSEventModifierFlags::Shift) { mods |= SHIFT; }
    if flags.contains(NSEventModifierFlags::Command) { mods |= CMD; }
    if code == key::ESCAPE && mods == 0 {
        say("");
        stop_recording();
        refresh();
        return true;
    }
    if (code == key::DELETE || code == key::FORWARD_DELETE) && mods == 0 {
        crate::with_desk(move |d| {
            d.set_custom_binding(c, None);
        });
        say(&format!("“{}” has no shortcut now.", c.title()));
        stop_recording();
        crate::ffi::main_async(refresh);
        return true;
    }
    let combo = Combo::new(code, mods);
    if let Some(problem) = combo.problem() {
        say(problem); // keep recording so they can try again
        return true;
    }
    stop_recording();
    crate::with_desk(move |d| {
        let msg = match d.set_custom_binding(c, Some(combo)) {
            Some(other) => format!("{} was used by “{}”, which now has no shortcut.", combo.spelled(), other.title()),
            None => format!("“{}” is now {}.", c.title(), combo.spelled()),
        };
        crate::ffi::main_async(move || {
            say(&msg);
            refresh();
        });
    });
    true
}

fn reset(p: Preset) {
    stop_recording();
    crate::with_desk(move |d| d.reset_custom_bindings(p));
    say(&format!("Reset to the default ({}) shortcuts.", p.label()));
    crate::ffi::main_async(refresh);
}

fn closed() {
    stop_recording();
    NSApplication::sharedApplication(mtm()).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    say("");
}
