//! User preferences in NSUserDefaults. Keys and values match the Swift version, so
//! settings (including custom shortcuts) carry over.

use crate::shortcuts::{Combo, Command, Preset};
use objc2_foundation::{NSData, NSString, NSUserDefaults};
use std::collections::HashMap;

fn defaults() -> objc2::rc::Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

fn key(k: &str) -> objc2::rc::Retained<NSString> {
    NSString::from_str(k)
}

pub fn string(k: &str) -> Option<String> {
    defaults().stringForKey(&key(k)).map(|s| s.to_string())
}

pub fn set_string(k: &str, v: &str) {
    unsafe { defaults().setObject_forKey(Some(&NSString::from_str(v)), &key(k)) };
}

pub fn bool(k: &str) -> bool {
    defaults().boolForKey(&key(k))
}

pub fn set_bool(k: &str, v: bool) {
    defaults().setBool_forKey(v, &key(k));
}

pub fn double(k: &str) -> f64 {
    defaults().doubleForKey(&key(k))
}

pub fn set_double(k: &str, v: f64) {
    defaults().setDouble_forKey(v, &key(k));
}

fn data(k: &str) -> Option<Vec<u8>> {
    defaults().dataForKey(&key(k)).map(|d| d.to_vec())
}

fn set_data(k: &str, v: Option<&[u8]>) {
    let d = defaults();
    match v {
        Some(bytes) => unsafe { d.setObject_forKey(Some(&NSData::with_bytes(bytes)), &key(k)) },
        None => d.removeObjectForKey(&key(k)),
    }
}

// MARK: Typed settings

pub fn preset() -> Preset {
    if let Some(p) = string("keymapPreset").as_deref().and_then(Preset::from_raw) {
        return p;
    }
    Preset::Shift
}

pub fn set_preset(p: Preset) {
    set_string("keymapPreset", p.raw());
}

pub fn custom_bindings() -> Option<HashMap<Command, Combo>> {
    let bytes = data("customBindings")?;
    let raw: HashMap<String, Combo> = serde_json::from_slice(&bytes).ok()?;
    let map: HashMap<Command, Combo> = raw.into_iter().filter_map(|(k, v)| Command::from_raw(&k).map(|c| (c, v))).collect();
    // Commands that existed when this set was saved. Sets saved before this list was kept
    // knew every command except the ones added since.
    let known: Vec<Command> = match data("customBindingsKnown").and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok()) {
        Some(names) => names.iter().filter_map(|n| Command::from_raw(n)).collect(),
        None => Command::ALL.iter().copied().filter(|c| !ADDED_AFTER_CUSTOM.contains(c)).collect(),
    };
    Some(with_new_defaults(map, &known))
}

/// Commands added after custom shortcuts existed (for sets saved before
/// `customBindingsKnown` was kept).
const ADDED_AFTER_CUSTOM: [Command; 1] = [Command::ShowDesktop];

/// Gives commands that are newer than a custom set their default shortcut, unless that
/// combination is already taken. Shortcuts the user removed stay removed.
pub fn with_new_defaults(mut map: HashMap<Command, Combo>, known: &[Command]) -> HashMap<Command, Combo> {
    let defaults = Preset::Shift.bindings();
    for &c in Command::ALL {
        if known.contains(&c) || map.contains_key(&c) {
            continue;
        }
        if let Some(&combo) = defaults.get(&c) {
            if !map.values().any(|v| *v == combo) {
                map.insert(c, combo);
            }
        }
    }
    map
}

pub fn set_custom_bindings(map: Option<&HashMap<Command, Combo>>) {
    match map {
        Some(m) => {
            let raw: HashMap<&str, Combo> = m.iter().map(|(c, v)| (c.raw(), *v)).collect();
            set_data("customBindings", serde_json::to_vec(&raw).ok().as_deref());
            let known: Vec<&str> = Command::ALL.iter().map(|c| c.raw()).collect();
            set_data("customBindingsKnown", serde_json::to_vec(&known).ok().as_deref());
        }
        None => {
            set_data("customBindings", None);
            set_data("customBindingsKnown", None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_commands_get_their_default_unless_taken() {
        let defaults = Preset::Shift.bindings();
        let mut map = defaults.clone();
        map.remove(&Command::ShowDesktop);
        map.remove(&Command::Center); // the user removed this one on purpose
        let known: Vec<Command> = Command::ALL.iter().copied().filter(|c| *c != Command::ShowDesktop).collect();
        let filled = with_new_defaults(map.clone(), &known);
        assert_eq!(filled.get(&Command::ShowDesktop), defaults.get(&Command::ShowDesktop), "new command gets its default");
        assert_eq!(filled.get(&Command::Center), None, "a removed shortcut stays removed");
        // Already taken by something else: left without a shortcut.
        map.insert(Command::Center, defaults[&Command::ShowDesktop]);
        let filled = with_new_defaults(map, &known);
        assert_eq!(filled.get(&Command::ShowDesktop), None);
    }
}

/// Which modifier keys turn scrolling into strip scrolling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollModifier {
    CtrlOpt,
    Shift,
    Option,
    Command,
    Control,
}

impl ScrollModifier {
    pub const ALL: [ScrollModifier; 5] =
        [ScrollModifier::CtrlOpt, ScrollModifier::Shift, ScrollModifier::Option, ScrollModifier::Command, ScrollModifier::Control];
    pub fn raw(self) -> &'static str {
        match self {
            ScrollModifier::CtrlOpt => "ctrlOpt",
            ScrollModifier::Shift => "shift",
            ScrollModifier::Option => "option",
            ScrollModifier::Command => "command",
            ScrollModifier::Control => "control",
        }
    }
    pub fn spelled(self) -> &'static str {
        match self {
            ScrollModifier::CtrlOpt => "⌃ Control + ⌥ Option",
            ScrollModifier::Shift => "⇧ Shift",
            ScrollModifier::Option => "⌥ Option",
            ScrollModifier::Command => "⌘ Command",
            ScrollModifier::Control => "⌃ Control",
        }
    }
    /// What holding this while scrolling normally does, which Reel takes over while tiling.
    pub fn caveat(self) -> Option<&'static str> {
        match self {
            ScrollModifier::CtrlOpt => None,
            ScrollModifier::Shift => Some("⇧ Shift + scroll normally scrolls sideways in apps."),
            ScrollModifier::Option => Some("⌥ Option + scroll is used by some apps."),
            ScrollModifier::Command => Some("⌘ Command + scroll zooms in some apps."),
            ScrollModifier::Control => Some("⌃ Control + scroll may trigger macOS screen zoom."),
        }
    }
    pub fn flags(self) -> u64 {
        use crate::ffi::*;
        match self {
            ScrollModifier::CtrlOpt => kCGEventFlagMaskControl | kCGEventFlagMaskAlternate,
            ScrollModifier::Shift => kCGEventFlagMaskShift,
            ScrollModifier::Option => kCGEventFlagMaskAlternate,
            ScrollModifier::Command => kCGEventFlagMaskCommand,
            ScrollModifier::Control => kCGEventFlagMaskControl,
        }
    }
}

pub fn scroll_modifier() -> ScrollModifier {
    let raw = string("scrollModifier").unwrap_or_default();
    ScrollModifier::ALL.into_iter().find(|m| m.raw() == raw).unwrap_or(ScrollModifier::CtrlOpt)
}

/// Animation speed multiplier (1 = the web demo's feel).
pub fn animation_speed() -> f64 {
    let v = double("animationSpeed");
    if v > 0.0 { v } else { 1.0 }
}

pub fn scroll_speed() -> f64 {
    let v = double("scrollSpeed");
    if v > 0.0 { v } else { 2.0 }
}

/// When the overview appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverviewMode {
    Auto,
    Always,
    Off,
}

impl OverviewMode {
    pub const ALL: [OverviewMode; 3] = [OverviewMode::Auto, OverviewMode::Always, OverviewMode::Off];
    pub fn raw(self) -> &'static str {
        match self { OverviewMode::Auto => "auto", OverviewMode::Always => "always", OverviewMode::Off => "off" }
    }
    pub fn label(self) -> &'static str {
        match self { OverviewMode::Auto => "On Change", OverviewMode::Always => "Always", OverviewMode::Off => "Off" }
    }
}

pub fn overview_mode() -> OverviewMode {
    let raw = string("overviewMode").unwrap_or_default();
    OverviewMode::ALL.into_iter().find(|m| m.raw() == raw).unwrap_or(OverviewMode::Auto)
}
