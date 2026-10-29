//! Owns the strip for one display and pushes its layout onto real windows.
//!
//! Animation model: the scroll offset and each column's strip-space x are springs. Every
//! frame each column's screen x = its x − offset (clamped to a "parking" slot when off
//! screen), and only windows whose rounded position changed are posted to the mover
//! threads. Sizes are not animated: resizing another app every frame makes it re-layout
//! its content continuously, so sizes jump to their final value and positions glide.

use crate::ax::{self, Element};
use crate::geometry::{Display, Rect};
use crate::layout_store::{self, SavedColumn, SavedStrip, WindowDescriptor};
use crate::overlays::{Overview, Ring, Snapshot, TabBar, Tile};
use crate::spring::{Spring, COLUMN_OMEGA, SCROLL_OMEGA};
use crate::strip::{ColId, Strip, WinId};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

/// How much of a scrolled-away window stays on screen. macOS tends to pull windows back
/// if they're placed entirely off every display.
const PEEK: f64 = 2.0;
/// While fingers are down the strip chases them on a very stiff spring.
const DRAG_RESPONSE: f64 = 0.06;
/// Experiment "softer swipe tracking": filters uneven scroll events more.
const SMOOTH_DRAG_RESPONSE: f64 = 0.13;

/// A real window Reel manages.
#[derive(Clone)]
pub struct Win {
    pub el: Element,
    pub pid: i32,
    pub app: String,
    pub bundle_id: Option<String>,
}

impl Win {
    pub fn new(el: Element, pid: i32) -> Win {
        el.set_timeout(0.3); // a hung app must not freeze Reel
        let info = crate::apps::info(pid);
        Win { el, pid, app: info.name, bundle_id: info.bundle_id }
    }

    pub fn title(&self) -> String {
        self.el.title()
    }

    /// The window no longer exists: AX says so, or its whole app has quit (AX then gives a
    /// different error, which used to leave "ghost" columns behind).
    pub fn is_gone(&self) -> bool {
        let process_gone = unsafe { libc::kill(self.pid, 0) } != 0;
        process_gone || self.el.is_gone()
    }

    pub fn descriptor(&self) -> WindowDescriptor {
        let size = self.el.size().unwrap_or_default();
        WindowDescriptor { bundle_id: self.bundle_id.clone(), pid: self.pid, title: self.title(), width: size.width, height: size.height }
    }

    /// Brings the window forward and gives it keyboard focus.
    pub fn focus(&self) {
        let app = Element::application(self.pid);
        app.set_timeout(0.3);
        app.set_bool(ax::FRONTMOST, true);
        self.el.set_bool(ax::MAIN, true);
        self.el.perform(ax::RAISE);
    }
}

thread_local! {
    /// After Reel moves focus itself the system's focus notifications lag behind; they're
    /// ignored briefly so rapid key presses aren't undone by stale events.
    static FOCUS_GUARD: Cell<Option<Instant>> = const { Cell::new(None) };
    static NEXT_WIN: Cell<u64> = const { Cell::new(1) };
    /// Post-focus checks still scheduled (the tests wait for these).
    static PENDING_CHECKS: Cell<u32> = const { Cell::new(0) };
}

pub fn pending_checks() -> u32 {
    PENDING_CHECKS.with(|p| p.get())
}

pub fn arm_focus_guard() {
    FOCUS_GUARD.with(|g| g.set(Some(Instant::now() + std::time::Duration::from_millis(400))));
}

pub fn focus_guard_armed() -> bool {
    FOCUS_GUARD.with(|g| g.get().is_some_and(|t| Instant::now() < t))
}

pub fn new_win_id() -> WinId {
    NEXT_WIN.with(|n| {
        let id = n.get();
        n.set(id + 1);
        WinId(id)
    })
}

/// Where taps on overlays go: (display key, what happened).
pub type OverlayAction = Rc<dyn Fn(&str, OverlayEvent)>;

pub enum OverlayEvent {
    Select { column: usize, row: usize },
    Tab { column: ColId, index: usize },
}

pub struct StripController {
    pub strip: Strip,
    pub display: Display,
    pub wins: HashMap<WinId, Win>,
    pub overview: Rc<Overview>,
    pub show_border: bool,
    pub dim_inactive: bool,
    on_overlay: OverlayAction,

    offset: Spring,
    /// "Show desktop": 0 = windows in place, 1 = slid off to the screen edges.
    desktop: Spring,
    column_x: HashMap<ColId, Spring>,
    applied: HashMap<WinId, (f64, f64)>,
    row_y: HashMap<WinId, f64>,
    last_pushed: HashMap<WinId, (f64, f64)>,
    tab_bars: HashMap<ColId, TabBar>,
    border: Option<Ring>,
    veils: HashMap<WinId, Ring>,
    drag_raw: Option<f64>,
    neighbor_left: bool,
    neighbor_right: bool,
    pub animating: bool,
    last_overview_offset: f64,
    primary_height: f64,
    // Per-animation timing, logged when it settles.
    stat_frames: u32,
    stat_moves: u32,
    stat_total: f64,
    stat_max: f64,
    /// Experiment "steady 60 Hz": when real windows were last sent positions, and whether
    /// this frame skips sending.
    last_ax_post: Instant,
    skip_ax: bool,
    /// Experiment "focus after the glide": focus the focused window once moving stops.
    focus_pending: bool,
    /// Last size sent to each window during live resizing.
    last_sized: HashMap<WinId, (f64, f64)>,
    /// Width animations: window → (spring over the width, final height).
    width_anim: HashMap<WinId, (Spring, f64)>,
    /// Sizes an app settled on instead of the exact one asked for (Terminal snaps heights to
    /// whole text rows): window → (size asked for, size it chose). Accepted until Reel asks
    /// for a different size, so Reel doesn't keep "correcting" it.
    settled: HashMap<WinId, (Size, Size)>,
    /// Experiment "pause enhanced UI": apps whose AXEnhancedUserInterface Reel turned off.
    eui_paused: HashSet<i32>,
}

impl StripController {
    pub fn new(display: Display, gap: f64, primary_height: f64, on_overlay: OverlayAction, overview: Rc<Overview>) -> Self {
        let mut strip = Strip::default();
        strip.gap = gap;
        strip.viewport = (display.usable.w, display.usable.h);
        overview.set_usable(display.appkit_usable);
        StripController {
            strip,
            display,
            wins: HashMap::new(),
            overview,
            show_border: false,
            dim_inactive: false,
            on_overlay,
            offset: Spring::new(0.0, SCROLL_OMEGA),
            desktop: Spring::new(0.0, COLUMN_OMEGA),
            column_x: HashMap::new(),
            applied: HashMap::new(),
            row_y: HashMap::new(),
            last_pushed: HashMap::new(),
            tab_bars: HashMap::new(),
            border: None,
            veils: HashMap::new(),
            drag_raw: None,
            neighbor_left: false,
            neighbor_right: false,
            animating: false,
            last_overview_offset: f64::NAN,
            primary_height,
            stat_frames: 0,
            stat_moves: 0,
            stat_total: 0.0,
            stat_max: 0.0,
            last_ax_post: Instant::now(),
            skip_ax: false,
            focus_pending: false,
            last_sized: HashMap::new(),
            settled: HashMap::new(),
            width_anim: HashMap::new(),
            eui_paused: HashSet::new(),
        }
    }

    pub fn key(&self) -> &str {
        &self.display.key
    }

    pub fn usable(&self) -> Rect {
        self.display.usable
    }

    pub fn find(&self, el: &Element) -> Option<WinId> {
        self.wins.iter().find(|(_, w)| w.el == *el).map(|(id, _)| *id)
    }

    pub fn contains(&self, el: &Element) -> bool {
        self.find(el).is_some()
    }

    pub fn set_neighbors(&mut self, left: bool, right: bool) {
        self.neighbor_left = left;
        self.neighbor_right = right;
    }

    // MARK: Building

    /// (Re)builds the strip from the live windows assigned to this display. Keeps the
    /// current arrangement if there is one; otherwise restores `reference` (saved layout);
    /// otherwise orders windows left to right as they are now.
    pub fn rebuild(&mut self, live: Vec<(Element, i32, Rect)>, reference: Option<&SavedStrip>) {
        self.strip.viewport = (self.usable().w, self.usable().h);
        let mut current_x: HashMap<WinId, f64> = HashMap::new();

        if !self.strip.columns.is_empty() {
            // Keep structure: drop windows that are gone, append newcomers by position.
            let gone: Vec<WinId> = self.wins.iter().filter(|(_, w)| !live.iter().any(|(e, _, _)| *e == w.el)).map(|(id, _)| *id).collect();
            for id in gone {
                self.strip.remove_window(id);
                self.forget(id);
            }
            let mut newcomers: Vec<(Element, i32, Rect)> = live.into_iter().filter(|(e, _, _)| !self.contains(e)).collect();
            newcomers.sort_by(|a, b| a.2.x.total_cmp(&b.2.x));
            for (el, pid, frame) in newcomers {
                let id = new_win_id();
                current_x.insert(id, frame.x);
                self.wins.insert(id, Win::new(el, pid));
                let col = self.strip.new_column(vec![id], 0.5);
                self.strip.columns.push(col);
            }
        } else {
            let mut entries: Vec<(WinId, Rect)> = vec![];
            for (el, pid, frame) in live {
                let id = new_win_id();
                self.wins.insert(id, Win::new(el, pid));
                current_x.insert(id, frame.x);
                entries.push((id, frame));
            }
            if let Some(reference) = reference {
                let described: Vec<(WinId, WindowDescriptor)> = entries.iter().map(|(id, _)| (*id, self.wins[id].descriptor())).collect();
                let plan = layout_store::plan(reference, &described);
                let mut cols = vec![];
                for pc in plan.columns {
                    let mut c = self.strip.new_column(pc.items, pc.share);
                    c.tabbed = pc.tabbed;
                    c.row = pc.row;
                    cols.push(c);
                }
                let mut leftovers = plan.leftovers;
                leftovers.sort_by(|a, b| current_x[a].total_cmp(&current_x[b]));
                for id in leftovers {
                    cols.push(self.strip.new_column(vec![id], 0.5));
                }
                self.strip.columns = cols;
                self.strip.focus = plan.focus;
            } else {
                entries.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
                let cols: Vec<_> = entries.iter().map(|(id, _)| self.strip.new_column(vec![*id], 0.5)).collect();
                self.strip.columns = cols;
                self.strip.focus = 0;
            }
        }
        self.strip.focus = self.strip.focus.min(self.strip.columns.len().saturating_sub(1));
        if let Some(el) = Element::system_focused_window() {
            if let Some(id) = self.find(&el) {
                self.strip.focus_on(id);
            }
        }
        self.strip.scroll_focus_into_view();

        for w in self.wins.values() {
            crate::events::watch(&w.el, w.pid);
        }
        // New columns start where their window is now, so tiling glides into place.
        let base = self.usable().x - self.offset.value;
        for col in &self.strip.columns {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.column_x.entry(col.id) {
                if let Some(x) = col.windows.first().and_then(|w| current_x.get(w)) {
                    slot.insert(Spring::new(x - base, COLUMN_OMEGA));
                }
            }
        }
        self.commit(true, true);
    }

    pub fn stop(&mut self) {
        self.restore_enhanced_ui();
        self.width_anim.clear();
        self.last_sized.clear();
        self.focus_pending = false;
        self.animating = false;
        self.overview.close();
        self.tab_bars.clear();
        self.border = None;
        self.veils.clear();
        crate::mover::Mover::flush(std::time::Duration::from_millis(500));
        self.bring_parked_windows_back();
        self.strip.columns.clear();
        self.wins.clear();
        self.column_x.clear();
        self.applied.clear();
        self.row_y.clear();
        self.last_pushed.clear();
    }

    pub fn set_gap(&mut self, gap: f64) {
        self.strip.gap = gap;
        for c in &mut self.strip.columns {
            c.accepted_width = None;
        }
        self.applied.clear();
        self.strip.scroll_focus_into_view();
        self.commit(true, true);
    }

    // MARK: Commands

    fn prepare(&mut self) -> bool {
        self.prune_gone();
        // With "focus after the glide" the system still shows the old window as focused
        // until the move ends; don't let that undo Reel's own focus.
        if !focus_guard_armed() && !self.focus_pending {
            self.sync_focus_with_system();
        }
        match self.strip.focused_window().and_then(|id| self.wins.get(&id)) {
            Some(w) => crate::log::write(&format!(
                "  strip: column {}/{}, row {}/{}, window \"{} — {}\"",
                self.strip.focus + 1,
                self.strip.columns.len(),
                self.strip.columns[self.strip.focus].row + 1,
                self.strip.columns[self.strip.focus].windows.len(),
                w.app,
                w.title()
            )),
            None => crate::log::write("  strip is empty"),
        }
        !self.strip.columns.is_empty()
    }

    fn refuse(&self, reason: &str) {
        crate::log::write(&format!("  refused: {reason}"));
        beep();
    }

    pub fn focus_focused_window(&mut self) {
        arm_focus_guard();
        if self.animating && crate::experiments::on(crate::experiments::Exp::FocusAfterGlide) {
            self.focus_pending = true; // done when the move settles
            return;
        }
        self.focus_now();
    }

    /// Focuses at once, even with "focus after the glide". Used when a window is being moved
    /// into another column (stack, pop out, tabs): raised late, it would slide behind the
    /// windows already there and then pop to the front, which looks like flicker.
    fn focus_moved_window(&mut self) {
        arm_focus_guard();
        self.focus_pending = false;
        self.focus_now();
    }

    fn focus_now(&mut self) {
        if let Some(w) = self.strip.focused_window().and_then(|id| self.wins.get(&id)) {
            w.focus();
            // An app that comes to the front sometimes nudges its window a few points a
            // moment later (a cold first activation can take a second or more); check
            // again once that has happened.
            for delay in [0.35, 1.2] {
                let key = self.display.key.clone();
                PENDING_CHECKS.with(|p| p.set(p.get() + 1));
                crate::later(delay, move |d| {
                    PENDING_CHECKS.with(|p| p.set(p.get().saturating_sub(1)));
                    if let Some(s) = d.strips.iter_mut().find(|s| s.key() == key) {
                        if !s.animating {
                            s.verify_sizes();
                        }
                    }
                });
            }
        }
    }

    pub fn focus_column(&mut self, delta: isize) {
        if self.prepare() && self.strip.move_focus(delta) {
            self.commit(true, true);
            self.focus_focused_window();
        }
    }

    pub fn focus_in_stack(&mut self, delta: isize) {
        if self.prepare() && self.strip.move_focus_in_stack(delta) {
            self.commit(true, true);
            self.focus_focused_window();
        }
    }

    pub fn move_column(&mut self, delta: isize) {
        if self.prepare() && self.strip.move_focused_column(delta) {
            self.commit(true, true);
        }
    }

    pub fn move_in_stack(&mut self, delta: isize) {
        if self.prepare() && self.strip.move_focused_window_in_stack(delta) {
            self.commit(true, true);
        }
    }

    pub fn stack(&mut self, delta: isize) {
        if !self.prepare() {
            return;
        }
        if !self.strip.stack_focused_window(delta) {
            return self.refuse("stack: no column on that side");
        }
        self.commit(true, true);
        self.focus_moved_window();
    }

    pub fn expel(&mut self, delta: isize) {
        if !self.prepare() {
            return;
        }
        if !self.strip.expel_focused_window(delta) {
            return self.refuse("pop out: focused column has only one window");
        }
        self.commit(true, true);
        self.focus_moved_window();
    }

    pub fn toggle_tabbed(&mut self) {
        if !self.prepare() {
            return;
        }
        if !self.strip.toggle_tabbed() {
            return self.refuse("tabbed: focused column has only one window");
        }
        self.commit(true, true);
        self.focus_moved_window();
    }

    pub fn unstack_all(&mut self) {
        if self.strip.unstack_all() {
            self.commit(true, true);
        }
    }

    pub fn cycle_width(&mut self, forward: bool) {
        if !self.prepare() {
            return;
        }
        let before = self.strip.focused_column().map(|c| self.strip.width(c)).unwrap_or(0.0);
        self.strip.cycle_focused_width(forward);
        self.commit(true, true);
        // Fixed-width windows (e.g. System Settings) refuse every size; say so.
        let after = self.strip.focused_column().map(|c| self.strip.width(c)).unwrap_or(0.0);
        if (after - before).abs() < 1.0 {
            beep();
        }
    }

    pub fn adjust_width(&mut self, steps: i32) {
        if !self.prepare() {
            return;
        }
        if !self.strip.adjust_focused_width(steps) {
            return self.refuse("width already at its limit");
        }
        self.commit(true, true);
    }

    pub fn toggle_full_width(&mut self) {
        if self.prepare() {
            self.strip.toggle_full_width();
            self.commit(true, true);
        }
    }

    pub fn center(&mut self) {
        if self.prepare() {
            self.strip.center_focused();
            self.commit(true, true);
        }
    }

    pub fn focus_column_at(&mut self, index: usize) {
        if !self.prepare() {
            return;
        }
        if !self.strip.focus_column_at(index) {
            return self.refuse(&format!("no column {}", index + 1));
        }
        self.commit(true, true);
        self.focus_focused_window();
    }

    pub fn focus_last_column(&mut self) {
        let last = self.strip.columns.len().saturating_sub(1);
        self.focus_column_at(last);
    }

    /// A block in the overview or a tab was clicked.
    pub fn select(&mut self, column: usize, row: usize) {
        if column >= self.strip.columns.len() {
            return;
        }
        self.strip.focus = column;
        let c = &mut self.strip.columns[column];
        c.row = row.min(c.windows.len() - 1);
        self.strip.scroll_focus_into_view();
        self.commit(true, true);
        self.focus_focused_window();
    }

    pub fn select_tab(&mut self, column: ColId, index: usize) {
        if let Some(c) = self.strip.columns.iter().position(|c| c.id == column) {
            self.select(c, index);
        }
    }

    /// Gives keyboard focus to this strip's focused window (switching displays).
    pub fn activate(&mut self) {
        if !self.strip.columns.is_empty() {
            self.focus_focused_window();
        }
    }

    /// Removes the focused window so another display can take it.
    pub fn take_focused_window(&mut self) -> Option<Win> {
        if !self.prepare() {
            return None;
        }
        let id = self.strip.focused_window()?;
        self.strip.remove_window(id);
        let w = self.wins.get(&id).cloned();
        self.forget(id);
        self.commit(true, true);
        w
    }

    /// Inserts a window (new, restored, or from another display) beside focus.
    pub fn receive(&mut self, w: Win) {
        if !focus_guard_armed() {
            self.sync_focus_with_system();
        }
        crate::events::watch(&w.el, w.pid);
        let id = new_win_id();
        self.wins.insert(id, w);
        self.strip.insert_beside_focus(vec![id], 0.5);
        self.commit(true, true);
        self.focus_focused_window();
    }

    // MARK: Window events

    /// Returns true if the window belonged to this strip.
    pub fn remove(&mut self, el: &Element) -> bool {
        let focused_before = self.strip.focused_window();
        let mut gone: Vec<WinId> = self.find(el).into_iter().collect();
        if gone.is_empty() {
            // A destroyed element may no longer compare equal.
            gone = self.wins.iter().filter(|(_, w)| w.is_gone()).map(|(id, _)| *id).collect();
        }
        if gone.is_empty() {
            return false;
        }
        for id in &gone {
            self.strip.remove_window(*id);
            self.forget(*id);
        }
        self.commit(true, true);
        if focused_before.is_some_and(|f| gone.contains(&f)) {
            self.focus_focused_window();
        }
        true
    }

    /// The user focused a window here from outside Reel (click, Spotlight, Dock...):
    /// scroll to it, but don't pop the overview up.
    pub fn follow_system_focus(&mut self, el: &Element) {
        if focus_guard_armed() {
            return;
        }
        let Some(id) = self.find(el) else { return };
        if self.strip.focused_window() == Some(id) {
            return;
        }
        self.strip.focus_on(id);
        self.commit(true, false);
    }

    /// The user resized a window by hand: adopt its new width for the whole column.
    pub fn window_resized(&mut self, el: &Element) {
        if self.drag_raw.is_some() {
            return;
        }
        let Some(id) = self.find(el) else { return };
        let Some((c, _)) = self.strip.locate(id) else { return };
        let (Some(size), Some(applied)) = (el.size(), self.applied.get(&id).copied()) else { return };
        if (size.width - applied.0).abs() <= 2.0 {
            // Same width but a different height: the app changed it by itself (some do when
            // they come to the front). Heights belong to the layout, so put it back.
            let own_choice = self.settled.get(&id).is_some_and(|(_, got)| close(*got, (size.width, size.height)));
            if (size.height - applied.1).abs() > 1.0 && !own_choice {
                if let (Some(win), Some(&(x, y))) = (self.wins.get(&id), self.last_pushed.get(&id)) {
                    win.el.set_position(x, y);
                    win.el.set_size(applied.0, applied.1);
                    crate::log::write(&format!("  restored the height of \"{}\" ({} → {})", win.title(), size.height, applied.1));
                    if let Some(got) = win.el.size() {
                        if !close((got.width, got.height), applied) {
                            self.settled.insert(id, (applied, (got.width, got.height)));
                        }
                    }
                }
            }
            return;
        }
        self.strip.columns[c].accepted_width = Some(size.width);
        self.applied.insert(id, (size.width, applied.1));
        // While the user drags an edge (mouse button down), gliding would move the window under
        // the mouse, which made dragging the left edge jumpy. Keep the window's left edge where
        // the user has it (scrolling the strip under it if needed) and apply the rest at once.
        // Other size changes (apps, macOS adjusting a frame) are absorbed by a normal glide.
        let mouse_down = objc2_app_kit::NSEvent::pressedMouseButtons() & 1 != 0;
        let area = self.usable();
        let edges = self.strip.column_edges().0;
        match (mouse_down, el.position(), edges.get(c)) {
            (true, Some(p), Some(&cx)) => {
                self.strip.offset = (cx - (p.x - area.x)).clamp(0.0, self.strip.max_offset());
                if let Some(entry) = self.last_pushed.get_mut(&id) {
                    entry.0 = p.x.round();
                }
                self.commit(false, false);
            }
            _ => {
                self.strip.scroll_focus_into_view();
                self.commit(true, true);
            }
        }
    }

    /// An app quit: drop all of its windows at once.
    pub fn remove_app(&mut self, pid: i32) -> bool {
        let ids: Vec<WinId> = self.wins.iter().filter(|(_, w)| w.pid == pid).map(|(id, _)| *id).collect();
        if ids.is_empty() {
            return false;
        }
        let had_focus = self.strip.focused_window().is_some_and(|f| ids.contains(&f));
        for id in ids {
            self.strip.remove_window(id);
            self.forget(id);
        }
        self.commit(true, true);
        if had_focus {
            self.focus_focused_window();
        }
        true
    }

    // MARK: Interactive scrolling

    pub fn begin_drag(&mut self) {
        self.drag_raw = Some(self.offset.value);
        let smooth = crate::experiments::on(crate::experiments::Exp::SwipeSmoothing);
        self.offset.response = Some(if smooth { SMOOTH_DRAG_RESPONSE } else { DRAG_RESPONSE });
        self.overview.show();
        self.start_animating();
    }

    pub fn drag(&mut self, delta: f64) {
        let Some(raw) = self.drag_raw else { return };
        let raw = raw - delta; // content follows the fingers
        self.drag_raw = Some(raw);
        self.offset.target = self.rubber_band(raw);
        self.start_animating();
    }

    /// Lets go with the given finger velocity (points/s); coasts to a column boundary.
    pub fn end_drag(&mut self, velocity: f64) {
        if self.drag_raw.take().is_none() {
            return;
        }
        self.offset.response = None;
        let v = (-velocity).clamp(-8000.0, 8000.0);
        let rest = self.strip.resting_offset(self.offset.target + v * 0.2);
        self.strip.offset = rest;
        self.offset.target = rest;
        if reduce_motion() {
            self.offset.snap(); // no glide after letting go
        }
        // If the focused column ended up off screen, focus the first fully visible one.
        if let Some(col) = self.strip.focused_column() {
            let x = self.strip.column_edges().0[self.strip.focus];
            let visible = x >= rest - 1.0 && x + self.strip.width(col) <= rest + self.strip.viewport.0 + 1.0;
            if !visible {
                if let Some(i) = self.strip.first_fully_visible_column(rest) {
                    self.strip.focus = i;
                    self.focus_focused_window();
                }
            }
        }
        self.start_animating();
        self.update_overview(true);
    }

    /// Marks this strip as moving and makes sure the frame clock is running. Every path
    /// that starts an animation goes through here, so nothing can be left waiting for a
    /// clock that isn't ticking.
    fn start_animating(&mut self) {
        if !self.animating && crate::experiments::on(crate::experiments::Exp::PauseEnhancedUi) {
            self.pause_enhanced_ui();
        }
        self.animating = true;
        crate::ensure_clock();
    }

    /// Experiment "pause enhanced UI": while this accessibility mode is on, some apps
    /// (Chromium, Electron) animate or delay their own window moves. Off while Reel moves
    /// them, back on afterwards. `AXEnhancedUserInterface` is undocumented.
    fn pause_enhanced_ui(&mut self) {
        let pids: HashSet<i32> = self.wins.values().map(|w| w.pid).collect();
        for pid in pids {
            if self.eui_paused.contains(&pid) {
                continue;
            }
            let app = Element::application(pid);
            app.set_timeout(0.1);
            if app.bool("AXEnhancedUserInterface") == Some(true) {
                app.set_bool("AXEnhancedUserInterface", false);
                self.eui_paused.insert(pid);
            }
        }
    }

    fn restore_enhanced_ui(&mut self) {
        for pid in self.eui_paused.drain() {
            let app = Element::application(pid);
            app.set_timeout(0.1);
            app.set_bool("AXEnhancedUserInterface", true);
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_raw.is_some()
    }

    /// Past either end the strip follows with increasing resistance.
    fn rubber_band(&self, raw: f64) -> f64 {
        let limit = self.strip.viewport.0 * 0.2;
        let hi = self.strip.max_offset();
        if raw < 0.0 {
            -limit * (-raw / (2.0 * limit)).tanh()
        } else if raw > hi {
            hi + limit * ((raw - hi) / (2.0 * limit)).tanh()
        } else {
            raw
        }
    }

    // MARK: Layout

    /// Applies the strip's current state: sizes immediately, positions via springs.
    pub fn commit(&mut self, animated: bool, show_overview: bool) {
        let usable = self.usable();
        self.strip.viewport = (usable.w, usable.h);

        // Resize. If an app refuses the width (min/max size), the column follows what it
        // accepted and a second pass re-fits the rest of the stack.
        for _ in 0..2 {
            let mut refused = false;
            for ci in 0..self.strip.columns.len() {
                let col = self.strip.columns[ci].clone();
                let w = self.strip.width(&col);
                let mut accepted: Option<f64> = None;
                for (id, (ry, rh)) in col.windows.iter().zip(self.strip.rows(&col)) {
                    self.row_y.insert(*id, ry);
                    if self.applied.get(id) == Some(&(w, rh)) {
                        continue;
                    }
                    if self.settled.get(id).is_some_and(|(asked, got)| close(*asked, (w, rh)) && self.applied.get(id).is_some_and(|a| close(*a, *got))) {
                        continue; // the app already chose its own size for this request
                    }
                    // Experiment "live resizing": a width-only change animates instead of
                    // jumping (see `push_frame` and `finish_width_anims`).
                    if let Some(&(aw, ah)) = self.applied.get(id) {
                        let animate = animated && !reduce_motion() && (ah - rh).abs() <= 1.0 && (aw - w).abs() > 1.0
                            && crate::experiments::on(crate::experiments::Exp::LiveResize);
                        if animate {
                            let mut sp = self.width_anim.get(id).map(|a| a.0).unwrap_or_else(|| Spring::new(aw, COLUMN_OMEGA));
                            sp.target = w;
                            self.width_anim.insert(*id, (sp, rh));
                            self.applied.insert(*id, (w, rh));
                            continue;
                        }
                    }
                    let Some(win) = self.wins.get(id) else { continue };
                    // Move to the new row's top edge *before* resizing: macOS caps a
                    // window's height to the space below its current origin.
                    let mut x = self.last_pushed.get(id).map(|p| p.0).or_else(|| win.el.position().map(|p| p.x)).unwrap_or(usable.x).round();
                    // It caps the width to the room on the right as well, so a visible window
                    // that's growing near the right edge (e.g. the last column) slides left
                    // just enough first. Its glide continues from there.
                    let visible = x < usable.max_x() - PEEK - 1.0 && x + w > usable.x + PEEK + 1.0;
                    if visible && x + w > usable.max_x() {
                        x = (usable.max_x() - w).max(usable.x).round();
                        if let Some(sp) = self.column_x.get_mut(&col.id) {
                            sp.value = x - usable.x + self.offset.value;
                        }
                    }
                    let y = (usable.y + ry).round();
                    win.el.set_position(x, y);
                    self.last_pushed.insert(*id, (x, y));
                    win.el.set_size(w, rh);
                    let mut got = win.el.size().map(|s| (s.width, s.height)).unwrap_or((w, rh));
                    if got.1 < rh - 1.0 {
                        win.el.set_size(w, rh); // some apps settle a beat later
                        got = win.el.size().map(|s| (s.width, s.height)).unwrap_or((w, rh));
                    }
                    // Record what was accepted so a shortfall is retried next time.
                    self.applied.insert(*id, (got.0, if (got.1 - rh).abs() <= 1.0 { rh } else { got.1 }));
                    if close(got, (w, rh)) {
                        self.settled.remove(id);
                    } else {
                        self.settled.insert(*id, ((w, rh), got));
                    }
                    if (got.0 - w).abs() > 1.0 {
                        accepted = Some(accepted.unwrap_or(0.0).max(got.0));
                    }
                }
                if let Some(a) = accepted {
                    if Some(a) != self.strip.columns[ci].accepted_width {
                        self.strip.columns[ci].accepted_width = Some(a);
                        refused = true;
                    }
                }
            }
            if !refused {
                break;
            }
            self.strip.scroll_focus_into_view();
        }

        // Retarget springs.
        let (xs, _) = self.strip.column_edges();
        let live: Vec<ColId> = self.strip.columns.iter().map(|c| c.id).collect();
        for (col, x) in self.strip.columns.iter().zip(xs) {
            self.column_x.entry(col.id).or_insert_with(|| Spring::new(x, COLUMN_OMEGA)).target = x;
        }
        self.column_x.retain(|id, _| live.contains(id));
        if self.drag_raw.is_none() {
            self.offset.target = self.strip.offset;
        }

        // Tab bars for tabbed columns.
        let mtm = self.overview.mtm();
        for col in self.strip.columns.iter().filter(|c| c.shows_tabs()) {
            let key = self.display.key.clone();
            let cb = self.on_overlay.clone();
            let cid = col.id;
            let bar = self.tab_bars.entry(col.id).or_insert_with(|| TabBar::new(mtm, move |i| cb(&key, OverlayEvent::Tab { column: cid, index: i })));
            let titles: Vec<String> = col.windows.iter().map(|id| self.wins.get(id).map(|w| w.title()).unwrap_or_default()).collect();
            bar.update(&titles, col.row);
        }
        let tabbed: Vec<ColId> = self.strip.columns.iter().filter(|c| c.shows_tabs()).map(|c| c.id).collect();
        self.tab_bars.retain(|id, _| tabbed.contains(id));

        if !animated || reduce_motion() {
            self.offset.snap();
            for s in self.column_x.values_mut() {
                s.snap();
            }
            self.push_frame(true);
        } else {
            self.push_frame(false);
            self.start_animating();
        }
        self.update_overview(show_overview);
    }

    /// Redraws the overview for the current arrangement.
    fn update_overview(&mut self, show: bool) {
        let (xs, total) = self.strip.column_edges();
        let tiles = self
            .strip
            .columns
            .iter()
            .zip(xs)
            .enumerate()
            .map(|(ci, (c, x))| Tile {
                x,
                width: self.strip.width(c),
                rows: c.windows.len(),
                tabbed: c.shows_tabs(),
                focused_row: (ci == self.strip.focus).then_some(c.row),
            })
            .collect();
        self.overview.update(Snapshot { tiles, total, viewport: self.strip.viewport }, self.offset.value, show);
        self.last_overview_offset = self.offset.value;
    }

    /// Shows the desktop (windows part to the screen edges like curtains) or brings the
    /// windows back.
    pub fn set_desktop(&mut self, show: bool) {
        self.desktop.target = if show { 1.0 } else { 0.0 };
        if reduce_motion() {
            self.desktop.snap();
        }
        self.start_animating();
    }

    pub fn desktop_shown(&self) -> bool {
        self.desktop.target > 0.5
    }

    /// One animation frame. Returns whether anything is still moving.
    pub fn tick(&mut self, dt: f64) -> bool {
        self.offset.advance(dt);
        self.desktop.advance(dt);
        let mut settled = self.offset.is_settled() && self.desktop.is_settled();
        for s in self.column_x.values_mut() {
            s.advance(dt);
            settled &= s.is_settled();
        }
        for (sp, _) in self.width_anim.values_mut() {
            sp.advance(dt);
            settled &= sp.is_settled();
        }
        use crate::experiments::{on, Exp};
        self.skip_ax = on(Exp::Steady60) && !settled && self.last_ax_post.elapsed() < std::time::Duration::from_millis(15);
        self.push_frame(false);
        self.skip_ax = false;
        if settled && self.drag_raw.is_none() && !self.width_anim.is_empty() {
            self.finish_width_anims();
        }
        if settled && self.drag_raw.is_none() {
            self.animating = false;
            if !self.eui_paused.is_empty() {
                crate::mover::Mover::flush(std::time::Duration::from_millis(100));
                self.restore_enhanced_ui();
            }
            if std::mem::take(&mut self.focus_pending) {
                self.focus_focused_window();
            }
            self.verify_sizes();
            if self.stat_frames > 0 {
                crate::log::write(&format!(
                    "  animation: {} frames, {:.1} moves/frame, push avg {:.2} ms, max {:.2} ms",
                    self.stat_frames,
                    self.stat_moves as f64 / self.stat_frames as f64,
                    self.stat_total / self.stat_frames as f64 * 1000.0,
                    self.stat_max * 1000.0
                ));
            }
            self.stat_frames = 0;
            self.stat_moves = 0;
            self.stat_total = 0.0;
            self.stat_max = 0.0;
        }
        self.animating
    }

    // MARK: Width animation

    /// A width animation ended: make sure each window really has its final size, and if an
    /// app refused it (fixed or minimum widths), let the column follow what it accepted.
    fn finish_width_anims(&mut self) {
        crate::mover::Mover::flush(std::time::Duration::from_millis(150));
        let mut refused = false;
        for (id, (sp, h)) in std::mem::take(&mut self.width_anim) {
            self.last_sized.remove(&id);
            let Some(win) = self.wins.get(&id) else { continue };
            let Some(got) = win.el.size() else { continue };
            if (got.width - sp.target).abs() > 1.0 || (got.height - h).abs() > 1.0 {
                win.el.set_size(sp.target, h);
                let again = win.el.size().map(|s| s.width).unwrap_or(sp.target);
                if (again - sp.target).abs() > 1.0 {
                    if let Some(c) = self.strip.columns.iter_mut().find(|c| c.windows.contains(&id)) {
                        c.accepted_width = Some(again);
                        refused = true;
                    }
                    self.applied.insert(id, (again, h));
                }
            }
        }
        if refused {
            self.strip.scroll_focus_into_view();
            self.commit(true, false);
        }
    }

    /// After an animation settles, re-check the real size of every window that's on screen
    /// and fix any that drifted. macOS sometimes changes a window's height while it's parked
    /// at the screen edge; without this, Reel's own record of the size would hide that.
    pub fn verify_sizes(&mut self) {
        let area = self.usable();
        crate::mover::Mover::flush(std::time::Duration::from_millis(200));
        let mut fixed = 0;
        for col in self.strip.columns.clone() {
            let w = self.strip.width(&col);
            for (id, (ry, rh)) in col.windows.iter().zip(self.strip.rows(&col)) {
                let Some(&(px, py)) = self.last_pushed.get(id) else { continue };
                if px + w <= area.x + 40.0 || px >= area.max_x() - 40.0 {
                    continue; // parked
                }
                let Some(win) = self.wins.get(id) else { continue };
                let Some(frame) = win.el.frame() else { continue };
                let want_w = self.applied.get(id).map(|a| a.0).unwrap_or(w);
                let in_place = (frame.x - px).abs() <= 1.0 && (frame.y - py).abs() <= 1.0;
                let exact = (frame.h - rh).abs() <= 1.0 && (frame.w - want_w).abs() <= 1.0;
                let own_choice = self.settled.get(id).is_some_and(|(asked, got)| close(*asked, (want_w, rh)) && close(*got, (frame.w, frame.h)));
                if in_place && (exact || own_choice) {
                    continue;
                }
                let y = (area.y + ry).round();
                win.el.set_position(px, y);
                win.el.set_size(want_w, rh);
                if let Some(got) = win.el.size() {
                    if !close((got.width, got.height), (want_w, rh)) {
                        self.settled.insert(*id, ((want_w, rh), (got.width, got.height)));
                    }
                }
                fixed += 1;
            }
        }
        if fixed > 0 {
            crate::log::write(&format!("  corrected the size of {fixed} window(s) that drifted"));
        }
    }

    /// Posts windows' current animated positions (and moves tab bars / decor).
    fn push_frame(&mut self, force_all: bool) {
        let started = Instant::now();
        let area = self.usable();
        let full = self.display.full;
        if self.offset.value != self.last_overview_offset {
            self.overview.set_offset(self.offset.value);
            self.last_overview_offset = self.offset.value;
        }
        let decor_on = self.show_border || self.dim_inactive;
        let focused = self.strip.focused_window();
        let mut focused_rect = None;
        let mut others: Vec<(WinId, Rect)> = vec![];
        let mut moves = 0;
        for col in &self.strip.columns {
            let Some(sx) = self.column_x.get(&col.id) else { continue };
            let w = self.strip.width(col);
            let mut sx_screen = area.x + sx.value - self.offset.value;
            if self.desktop.value != 0.0 {
                // Columns left of the middle slide out to the left, the rest to the right.
                let out = if sx_screen + w / 2.0 < area.x + area.w / 2.0 { area.x - w - sx_screen } else { area.max_x() - sx_screen };
                sx_screen += out * self.desktop.value;
            }
            let (x, hidden) = self.park(sx_screen, w, area);
            for id in &col.windows {
                let y = if hidden { full.max_y() - PEEK } else { area.y + self.row_y.get(id).copied().unwrap_or(self.strip.gap) };
                let p = (x.round(), y.round());
                let size = self.width_anim.get(id).map(|(sp, h)| (sp.value.round(), *h));
                let resized = size.is_some() && self.last_sized.get(id) != size.as_ref();
                if (force_all || resized || self.last_pushed.get(id) != Some(&p)) && !self.skip_ax {
                    if let Some(win) = self.wins.get(id) {
                        crate::with_mover(|m| m.post_frame(win.pid, id.0, &win.el, p.0, p.1, size));
                        moves += 1;
                    }
                    self.last_pushed.insert(*id, p);
                    if let Some(s) = size {
                        self.last_sized.insert(*id, s);
                    }
                }
                if decor_on && !hidden {
                    if let Some(&(aw, ah)) = self.applied.get(id) {
                        let r = Rect::new(p.0, p.1, aw, ah);
                        let visible_w = r.max_x().min(area.max_x()) - r.x.max(area.x);
                        if visible_w > 40.0 && !(col.shows_tabs() && Some(*id) != col.focused_window()) {
                            if Some(*id) == focused { focused_rect = Some(r) } else { others.push((*id, r)) }
                        }
                    }
                }
            }
            if let Some(bar) = self.tab_bars.get(&col.id) {
                let rect = (!hidden).then(|| Rect::new(x, area.y + self.strip.gap, w, self.strip.tab_bar_height));
                bar.place(rect, area, self.primary_height);
            }
        }
        if moves > 0 {
            self.last_ax_post = Instant::now();
        }
        if decor_on {
            self.update_decor(focused_rect, others);
        }
        let spent = started.elapsed().as_secs_f64();
        self.stat_frames += 1;
        self.stat_moves += moves;
        self.stat_total += spent;
        self.stat_max = self.stat_max.max(spent);
    }

    fn update_decor(&mut self, focused: Option<Rect>, others: Vec<(WinId, Rect)>) {
        let mtm = self.overview.mtm();
        match (self.show_border, focused) {
            (true, Some(r)) => self.border.get_or_insert_with(|| Ring::new(mtm, true)).place(Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0), self.primary_height),
            _ => {
                if let Some(b) = &self.border {
                    b.hide();
                }
            }
        }
        if self.dim_inactive {
            let live: Vec<WinId> = others.iter().map(|o| o.0).collect();
            for (id, r) in others {
                self.veils.entry(id).or_insert_with(|| Ring::new(mtm, false)).place(r, self.primary_height);
            }
            self.veils.retain(|id, _| live.contains(id));
        } else {
            self.veils.clear();
        }
    }

    pub fn clear_decor(&mut self) {
        if !self.show_border {
            self.border = None;
        }
        if !self.dim_inactive {
            self.veils.clear();
        }
    }

    /// Where an off-screen column goes. Normally it waits at the nearest side edge with a
    /// sliver showing. If another display touches that edge the sliver would show up there,
    /// so instead it drops below the bottom edge (`hidden`).
    fn park(&self, x: f64, w: f64, area: Rect) -> (f64, bool) {
        if x < area.x {
            if self.neighbor_left {
                return (area.x, true);
            }
            return (x.max(area.x - w + PEEK), false);
        }
        if x + w > area.max_x() {
            if self.neighbor_right {
                return (area.x.max(area.max_x() - w), true);
            }
            return (x.min(area.max_x() - PEEK), false);
        }
        (x, false)
    }

    // MARK: Persistence

    pub fn snapshot(&self) -> SavedStrip {
        SavedStrip {
            columns: self
                .strip
                .columns
                .iter()
                .map(|c| SavedColumn {
                    windows: c.windows.iter().filter_map(|id| self.wins.get(id)).map(Win::descriptor).collect(),
                    share: c.share,
                    tabbed: c.tabbed,
                    row: c.row,
                })
                .collect(),
            focus: self.strip.focus,
        }
    }

    // MARK: Housekeeping

    fn forget(&mut self, id: WinId) {
        self.width_anim.remove(&id);
        self.last_sized.remove(&id);
        self.settled.remove(&id);
        self.wins.remove(&id);
        self.applied.remove(&id);
        self.row_y.remove(&id);
        self.last_pushed.remove(&id);
        self.veils.remove(&id);
    }

    /// Safety net for windows whose destroy notification we missed.
    fn prune_gone(&mut self) {
        let gone: Vec<WinId> = self.wins.iter().filter(|(_, w)| w.is_gone()).map(|(id, _)| *id).collect();
        for id in gone {
            self.strip.remove_window(id);
            self.forget(id);
        }
    }

    /// The user may have clicked another window since our last command.
    fn sync_focus_with_system(&mut self) {
        if let Some(el) = Element::system_focused_window() {
            if let Some(id) = self.find(&el) {
                self.strip.focus_on(id);
            }
        }
    }

    fn bring_parked_windows_back(&self) {
        let area = self.usable();
        for (n, w) in self.strip.all_windows().filter_map(|id| self.wins.get(&id)).enumerate() {
            let Some(f) = w.el.frame() else { continue };
            if f.overlap_area(&area.inset(40.0, 40.0)) > 0.0 {
                continue;
            }
            let step = (n % 8) as f64 * 24.0;
            let (x, y) = (area.x + 40.0 + step, area.y + 40.0 + step);
            w.el.set_position(x, y);
            let after = w.el.frame();
            crate::log::write(&format!("  brought back \"{}\": {:?} → wanted ({x}, {y}) → now {:?}", w.title(), (f.x, f.y), after.map(|a| (a.x, a.y))));
        }
    }
}

pub fn beep() {
    objc2_app_kit::NSBeep();
}

/// Moves are instant when the user turned animations off in Reel or asked macOS to reduce
/// motion.
fn reduce_motion() -> bool {
    !crate::spring::animations_on() || objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// Width and height.
type Size = (f64, f64);

/// Two sizes the same to within a point.
fn close(a: Size, b: Size) -> bool {
    (a.0 - b.0).abs() <= 1.0 && (a.1 - b.1).abs() <= 1.0
}
