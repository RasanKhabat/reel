//! Every bindable action, the built-in keymaps, and how key combinations are named and
//! validated. Raw names and JSON shapes match the Swift version so saved custom
//! shortcuts carry over.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Carbon modifier bits.
pub const CMD: u32 = 0x0100;
pub const SHIFT: u32 = 0x0200;
pub const OPTION: u32 = 0x0800;
pub const CONTROL: u32 = 0x1000;
pub const CTRL_OPT: u32 = CONTROL | OPTION;
pub const CTRL_OPT_SHIFT: u32 = CONTROL | OPTION | SHIFT;
pub const SHIFT_OPT: u32 = SHIFT | OPTION;

// Virtual key codes (US ANSI positions).
pub mod key {
    pub const A: u32 = 0x00; pub const S: u32 = 0x01; pub const D: u32 = 0x02; pub const F: u32 = 0x03;
    pub const H: u32 = 0x04; pub const G: u32 = 0x05; pub const Z: u32 = 0x06; pub const X: u32 = 0x07;
    pub const C: u32 = 0x08; pub const V: u32 = 0x09; pub const B: u32 = 0x0B; pub const Q: u32 = 0x0C;
    pub const W: u32 = 0x0D; pub const E: u32 = 0x0E; pub const R: u32 = 0x0F; pub const Y: u32 = 0x10;
    pub const T: u32 = 0x11; pub const N1: u32 = 0x12; pub const N2: u32 = 0x13; pub const N3: u32 = 0x14;
    pub const N4: u32 = 0x15; pub const N6: u32 = 0x16; pub const N5: u32 = 0x17; pub const EQUAL: u32 = 0x18;
    pub const N9: u32 = 0x19; pub const N7: u32 = 0x1A; pub const MINUS: u32 = 0x1B; pub const N8: u32 = 0x1C;
    pub const N0: u32 = 0x1D; pub const RIGHT_BRACKET: u32 = 0x1E; pub const O: u32 = 0x1F; pub const U: u32 = 0x20;
    pub const LEFT_BRACKET: u32 = 0x21; pub const I: u32 = 0x22; pub const P: u32 = 0x23; pub const RETURN: u32 = 0x24;
    pub const L: u32 = 0x25; pub const J: u32 = 0x26; pub const QUOTE: u32 = 0x27; pub const K: u32 = 0x28;
    pub const SEMICOLON: u32 = 0x29; pub const BACKSLASH: u32 = 0x2A; pub const COMMA: u32 = 0x2B;
    pub const SLASH: u32 = 0x2C; pub const N: u32 = 0x2D; pub const M: u32 = 0x2E; pub const PERIOD: u32 = 0x2F;
    pub const TAB: u32 = 0x30; pub const SPACE: u32 = 0x31; pub const GRAVE: u32 = 0x32; pub const DELETE: u32 = 0x33;
    pub const ESCAPE: u32 = 0x35; pub const F5: u32 = 0x60; pub const F6: u32 = 0x61; pub const F7: u32 = 0x62;
    pub const F3: u32 = 0x63; pub const F8: u32 = 0x64; pub const F9: u32 = 0x65; pub const F11: u32 = 0x67;
    pub const F10: u32 = 0x6D; pub const F12: u32 = 0x6F; pub const HOME: u32 = 0x73; pub const PAGE_UP: u32 = 0x74;
    pub const FORWARD_DELETE: u32 = 0x75; pub const F4: u32 = 0x76; pub const END: u32 = 0x77; pub const F2: u32 = 0x78;
    pub const PAGE_DOWN: u32 = 0x79; pub const F1: u32 = 0x7A; pub const LEFT: u32 = 0x7B; pub const RIGHT: u32 = 0x7C;
    pub const DOWN: u32 = 0x7D; pub const UP: u32 = 0x7E;
}

macro_rules! commands {
    ($($variant:ident = $raw:literal, $group:literal, $title:literal;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Command { $($variant),* }

        impl Command {
            pub const ALL: &'static [Command] = &[$(Command::$variant),*];
            pub fn raw(self) -> &'static str { match self { $(Command::$variant => $raw),* } }
            pub fn from_raw(s: &str) -> Option<Command> { match s { $($raw => Some(Command::$variant),)* _ => None } }
            pub fn group(self) -> &'static str { match self { $(Command::$variant => $group),* } }
            pub fn title(self) -> &'static str { match self { $(Command::$variant => $title),* } }
        }
    };
}

commands! {
    FocusLeft = "focusLeft", "Focus", "Focus column to the left";
    FocusRight = "focusRight", "Focus", "Focus column to the right";
    FocusUp = "focusUp", "Focus", "Focus window above (in a stack)";
    FocusDown = "focusDown", "Focus", "Focus window below (in a stack)";
    MoveLeft = "moveLeft", "Move", "Move column left";
    MoveRight = "moveRight", "Move", "Move column right";
    MoveUp = "moveUp", "Move", "Move window up in its stack";
    MoveDown = "moveDown", "Move", "Move window down in its stack";
    StackLeft = "stackLeft", "Stacks & tabs", "Stack window into the left column";
    StackRight = "stackRight", "Stacks & tabs", "Stack window into the right column";
    PopOutLeft = "popOutLeft", "Stacks & tabs", "Pop window out of its stack, to the left";
    PopOutRight = "popOutRight", "Stacks & tabs", "Pop window out of its stack, to the right";
    ToggleTabbed = "toggleTabbed", "Stacks & tabs", "Tabbed column on / off";
    UnstackAll = "unstackAll", "Stacks & tabs", "Unstack all (every window its own column)";
    WidthNext = "widthNext", "Width & position", "Cycle width forward (⅓ → ½ → ⅔ → full)";
    WidthPrevious = "widthPrevious", "Width & position", "Cycle width back";
    Narrower = "narrower", "Width & position", "Narrower (next size down)";
    Wider = "wider", "Width & position", "Wider (next size up)";
    FullWidth = "fullWidth", "Width & position", "Full width on / off";
    Center = "center", "Width & position", "Centre focused column";
    Column1 = "column1", "Jump", "Jump to column 1";
    Column2 = "column2", "Jump", "Jump to column 2";
    Column3 = "column3", "Jump", "Jump to column 3";
    Column4 = "column4", "Jump", "Jump to column 4";
    Column5 = "column5", "Jump", "Jump to column 5";
    Column6 = "column6", "Jump", "Jump to column 6";
    Column7 = "column7", "Jump", "Jump to column 7";
    Column8 = "column8", "Jump", "Jump to column 8";
    Column9 = "column9", "Jump", "Jump to column 9";
    LastColumn = "lastColumn", "Jump", "Jump to last column";
    FocusDisplayLeft = "focusDisplayLeft", "Displays", "Focus display to the left";
    FocusDisplayRight = "focusDisplayRight", "Displays", "Focus display to the right";
    SendToDisplayLeft = "sendToDisplayLeft", "Displays", "Send window to display on the left";
    SendToDisplayRight = "sendToDisplayRight", "Displays", "Send window to display on the right";
}

impl Command {
    pub fn column_number(self) -> Option<usize> {
        use Command::*;
        [Column1, Column2, Column3, Column4, Column5, Column6, Column7, Column8, Column9]
            .iter()
            .position(|c| *c == self)
            .map(|i| i + 1)
    }

    /// Commands in display order, grouped.
    pub fn groups() -> Vec<(&'static str, Vec<Command>)> {
        let mut out: Vec<(&'static str, Vec<Command>)> = vec![];
        for &c in Self::ALL {
            match out.iter_mut().find(|(g, _)| *g == c.group()) {
                Some((_, list)) => list.push(c),
                None => out.push((c.group(), vec![c])),
            }
        }
        out
    }
}

/// One key combination (Carbon key code + modifier mask).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Combo {
    #[serde(rename = "keyCode")]
    pub key_code: u32,
    pub modifiers: u32,
}

pub const MODIFIER_LEGEND: &str = "⌃ Control · ⌥ Option · ⇧ Shift · ⌘ Command";

impl Combo {
    pub const fn new(key_code: u32, modifiers: u32) -> Self {
        Combo { key_code, modifiers }
    }

    fn modifier_parts(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![];
        if self.modifiers & CONTROL != 0 { v.push(("⌃", "Control")); }
        if self.modifiers & OPTION != 0 { v.push(("⌥", "Option")); }
        if self.modifiers & SHIFT != 0 { v.push(("⇧", "Shift")); }
        if self.modifiers & CMD != 0 { v.push(("⌘", "Command")); }
        v
    }

    /// Compact form, e.g. "⌃⌥←".
    pub fn glyphs(&self) -> String {
        let mut s: String = self.modifier_parts().iter().map(|p| p.0).collect();
        s.push_str(&key_name(self.key_code).0);
        s
    }

    /// Every key spelled out, e.g. "⌃ Control + ⌥ Option + ← Left".
    pub fn spelled(&self) -> String {
        let mut parts: Vec<String> = self.modifier_parts().iter().map(|(g, n)| format!("{g} {n}")).collect();
        let (g, n) = key_name(self.key_code);
        parts.push(if g == n { n } else { format!("{g} {n}") });
        parts.join(" + ")
    }

    /// Why this combination can't be a global shortcut, or None if it can.
    pub fn problem(&self) -> Option<&'static str> {
        if self.modifiers & (CONTROL | OPTION | CMD) != 0 {
            return None;
        }
        if self.modifiers & SHIFT != 0 && is_non_printing(self.key_code) {
            return None;
        }
        if is_non_printing(self.key_code) && self.modifiers == 0 {
            return Some("A bare key would stop working everywhere else. Add ⌃ Control, ⌥ Option, ⇧ Shift or ⌘ Command.");
        }
        Some("That would block normal typing. Include ⌃ Control, ⌥ Option or ⌘ Command.")
    }
}

/// Keys that never type a character, so ⇧ alone is an acceptable modifier for them.
pub fn is_non_printing(code: u32) -> bool {
    use key::*;
    matches!(code, LEFT | RIGHT | UP | DOWN | HOME | END | PAGE_UP | PAGE_DOWN | FORWARD_DELETE
        | F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12)
}

/// Glyph and spoken name for a key.
pub fn key_name(code: u32) -> (String, String) {
    use key::*;
    let pair = |g: &str, n: &str| (g.to_string(), n.to_string());
    match code {
        LEFT => pair("←", "Left"), RIGHT => pair("→", "Right"), UP => pair("↑", "Up"), DOWN => pair("↓", "Down"),
        LEFT_BRACKET => pair("[", "Left Bracket"), RIGHT_BRACKET => pair("]", "Right Bracket"),
        MINUS => pair("-", "Minus"), EQUAL => pair("=", "Equals"), COMMA => pair(",", "Comma"),
        PERIOD => pair(".", "Period"), SLASH => pair("/", "Slash"), BACKSLASH => pair("\\", "Backslash"),
        SEMICOLON => pair(";", "Semicolon"), QUOTE => pair("'", "Quote"), GRAVE => pair("`", "Backtick"),
        SPACE => pair("␣", "Space"), RETURN => pair("↩", "Return"), TAB => pair("⇥", "Tab"),
        ESCAPE => pair("⎋", "Escape"), DELETE => pair("⌫", "Delete"), FORWARD_DELETE => pair("⌦", "Forward Delete"),
        HOME => pair("↖", "Home"), END => pair("↘", "End"), PAGE_UP => pair("⇞", "Page Up"), PAGE_DOWN => pair("⇟", "Page Down"),
        F1 => pair("F1", "F1"), F2 => pair("F2", "F2"), F3 => pair("F3", "F3"), F4 => pair("F4", "F4"),
        F5 => pair("F5", "F5"), F6 => pair("F6", "F6"), F7 => pair("F7", "F7"), F8 => pair("F8", "F8"),
        F9 => pair("F9", "F9"), F10 => pair("F10", "F10"), F11 => pair("F11", "F11"), F12 => pair("F12", "F12"),
        _ => {
            const LETTERS: [(u32, &str); 36] = [
                (key::A, "A"), (key::B, "B"), (key::C, "C"), (key::D, "D"), (key::E, "E"), (key::F, "F"),
                (key::G, "G"), (key::H, "H"), (key::I, "I"), (key::J, "J"), (key::K, "K"), (key::L, "L"),
                (key::M, "M"), (key::N, "N"), (key::O, "O"), (key::P, "P"), (key::Q, "Q"), (key::R, "R"),
                (key::S, "S"), (key::T, "T"), (key::U, "U"), (key::V, "V"), (key::W, "W"), (key::X, "X"),
                (key::Y, "Y"), (key::Z, "Z"), (key::N0, "0"), (key::N1, "1"), (key::N2, "2"), (key::N3, "3"),
                (key::N4, "4"), (key::N5, "5"), (key::N6, "6"), (key::N7, "7"), (key::N8, "8"), (key::N9, "9"),
            ];
            match LETTERS.iter().find(|(c, _)| *c == code) {
                Some((_, s)) => pair(s, s),
                None => (format!("#{code}"), format!("Key {code}")),
            }
        }
    }
}

/// The built-in keymap (⇧ arrows), or the user's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Shift,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 2] = [Preset::Shift, Preset::Custom];
    pub fn raw(self) -> &'static str {
        match self { Preset::Shift => "shift", Preset::Custom => "custom" }
    }
    pub fn from_raw(s: &str) -> Option<Preset> {
        match s {
            // The ⌃⌥-arrows layout was retired; people who had it get the default.
            "ctrlOpt" => Some(Preset::Shift),
            _ => Self::ALL.into_iter().find(|p| p.raw() == s),
        }
    }
    pub fn label(self) -> &'static str {
        match self { Preset::Shift => "⇧ Arrows", Preset::Custom => "Custom" }
    }

    /// The default keymap: ⇧ + arrows to focus, ⌥⇧ + arrows to move. Letter, number and
    /// bracket commands use ⌃⌥, because ⇧ + letter / ⇧ + bracket is ordinary typing.
    pub fn bindings(self) -> HashMap<Command, Combo> {
        use Command::*;
        let (focus, mv) = (SHIFT, SHIFT_OPT);
        let (co, cos) = (CTRL_OPT, CTRL_OPT_SHIFT);
        let k = Combo::new;
        let mut b = HashMap::from([
            (FocusLeft, k(key::LEFT, focus)), (FocusRight, k(key::RIGHT, focus)),
            (FocusUp, k(key::UP, focus)), (FocusDown, k(key::DOWN, focus)),
            (MoveLeft, k(key::LEFT, mv)), (MoveRight, k(key::RIGHT, mv)),
            (MoveUp, k(key::UP, mv)), (MoveDown, k(key::DOWN, mv)),
            (StackLeft, k(key::LEFT_BRACKET, co)), (StackRight, k(key::RIGHT_BRACKET, co)),
            (PopOutLeft, k(key::LEFT_BRACKET, cos)), (PopOutRight, k(key::RIGHT_BRACKET, cos)),
            (ToggleTabbed, k(key::T, co)), (UnstackAll, k(key::U, co)),
            (WidthNext, k(key::R, co)), (WidthPrevious, k(key::R, cos)),
            (Narrower, k(key::MINUS, co)), (Wider, k(key::EQUAL, co)),
            (FullWidth, k(key::F, co)), (Center, k(key::C, co)),
            (LastColumn, k(key::N0, co)),
            (FocusDisplayLeft, k(key::COMMA, co)), (FocusDisplayRight, k(key::PERIOD, co)),
            (SendToDisplayLeft, k(key::COMMA, cos)), (SendToDisplayRight, k(key::PERIOD, cos)),
        ]);
        let digits = [key::N1, key::N2, key::N3, key::N4, key::N5, key::N6, key::N7, key::N8, key::N9];
        let cols = [Column1, Column2, Column3, Column4, Column5, Column6, Column7, Column8, Column9];
        for (c, d) in cols.into_iter().zip(digits) {
            b.insert(c, k(d, co));
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_covers_everything_without_clashes() {
        let b = Preset::Shift.bindings();
        assert_eq!(b.len(), Command::ALL.len());
        let uniq: HashSet<_> = b.values().collect();
        assert_eq!(uniq.len(), b.len(), "a combo is bound twice");
        assert!(b.values().all(|c| c.problem().is_none()));
    }

    #[test]
    fn default_layout() {
        let s = Preset::Shift.bindings();
        assert_eq!(s[&Command::FocusLeft].glyphs(), "⇧←");
        assert_eq!(s[&Command::MoveRight].glyphs(), "⌥⇧→");
        assert_eq!(s[&Command::StackLeft].glyphs(), "⌃⌥[");
        assert_eq!(Preset::from_raw("ctrlOpt"), Some(Preset::Shift), "the retired layout maps to the default");
    }

    #[test]
    fn spelled_names() {
        assert_eq!(Combo::new(key::RIGHT_BRACKET, CTRL_OPT_SHIFT).spelled(), "⌃ Control + ⌥ Option + ⇧ Shift + ] Right Bracket");
        assert_eq!(Combo::new(key::R, CTRL_OPT).spelled(), "⌃ Control + ⌥ Option + R");
        assert_eq!(Combo::new(key::LEFT, SHIFT).spelled(), "⇧ Shift + ← Left");
    }

    #[test]
    fn validation() {
        assert!(Combo::new(key::R, 0).problem().is_some());
        assert!(Combo::new(key::R, SHIFT).problem().is_some());
        assert!(Combo::new(key::LEFT, 0).problem().is_some());
        assert!(Combo::new(key::LEFT, SHIFT).problem().is_none());
        assert!(Combo::new(key::R, CMD | OPTION).problem().is_none());
    }

    #[test]
    fn json_matches_swift_shape_and_raw_names_round_trip() {
        let c: Combo = serde_json::from_str(r#"{"keyCode":123,"modifiers":512}"#).unwrap();
        assert_eq!(c, Combo::new(key::LEFT, SHIFT));
        for &cmd in Command::ALL {
            assert_eq!(Command::from_raw(cmd.raw()), Some(cmd));
        }
        assert_eq!(Command::Column7.column_number(), Some(7));
        assert_eq!(Command::groups().iter().map(|g| g.1.len()).sum::<usize>(), Command::ALL.len());
    }
}
