//! Animation experiments: independent switches, all off by default, shown in the panel's
//! "Animation experiments" section. Each is read on the hot path, so the switches live in
//! an atomic bit set (safe from the mover threads too) and are saved in NSUserDefaults as
//! `exp.<key>`.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exp {
    LiveResize,
    PauseEnhancedUi,
    FocusAfterGlide,
    PriorityMovers,
    Eased,
    SwipeSmoothing,
    Steady60,
}

impl Exp {
    pub const ALL: [Exp; 7] = [
        Exp::LiveResize,
        Exp::PauseEnhancedUi,
        Exp::FocusAfterGlide,
        Exp::PriorityMovers,
        Exp::Eased,
        Exp::SwipeSmoothing,
        Exp::Steady60,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Exp::LiveResize => "liveResize",
            Exp::PauseEnhancedUi => "pauseEnhancedUI",
            Exp::FocusAfterGlide => "focusAfterGlide",
            Exp::PriorityMovers => "priorityMovers",
            Exp::Eased => "eased",
            Exp::SwipeSmoothing => "swipeSmoothing",
            Exp::Steady60 => "steady60",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Exp::LiveResize => "Live resizing",
            Exp::PauseEnhancedUi => "Pause apps' enhanced UI while moving",
            Exp::FocusAfterGlide => "Focus after the glide",
            Exp::PriorityMovers => "High-priority move threads",
            Exp::Eased => "Eased curve instead of spring",
            Exp::SwipeSmoothing => "Softer swipe tracking",
            Exp::Steady60 => "Steady 60 Hz window moves",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Exp::LiveResize => "Width changes animate on the real window instead of jumping.",
            Exp::PauseEnhancedUi => "Some apps (Chrome, Electron) animate their own moves while this accessibility mode is on. Undocumented.",
            Exp::FocusAfterGlide => "Switching apps mid-move makes them redraw; focus the new window once it has arrived.",
            Exp::PriorityMovers => "Runs the threads that move windows at the highest scheduling priority.",
            Exp::Eased => "A fixed-length ease instead of the spring; same speed setting.",
            Exp::SwipeSmoothing => "Smooths uneven trackpad events at the cost of a little lag behind the fingers.",
            Exp::Steady60 => "Sends window moves 60 times a second, evenly, instead of every 120 Hz frame.",
        }
    }

    pub fn from_key(key: &str) -> Option<Exp> {
        Exp::ALL.into_iter().find(|e| e.key() == key)
    }

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

static FLAGS: AtomicU32 = AtomicU32::new(0);

/// Reads the saved switches (call once at launch).
pub fn load() {
    let mut bits = 0;
    for e in Exp::ALL {
        if crate::prefs::bool(&format!("exp.{}", e.key())) {
            bits |= e.bit();
        }
    }
    FLAGS.store(bits, Ordering::Relaxed);
}

pub fn on(e: Exp) -> bool {
    FLAGS.load(Ordering::Relaxed) & e.bit() != 0
}

/// Sets a switch. `save` is false for test-mode overrides.
pub fn set(e: Exp, value: bool, save: bool) {
    if value {
        FLAGS.fetch_or(e.bit(), Ordering::Relaxed);
    } else {
        FLAGS.fetch_and(!e.bit(), Ordering::Relaxed);
    }
    if save {
        crate::prefs::set_bool(&format!("exp.{}", e.key()), value);
    }
    crate::log::write(&format!("experiment {} {}", e.key(), if value { "on" } else { "off" }));
}
