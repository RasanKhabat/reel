//! Coordinates one strip per display: routes window events, keyboard commands and swipes
//! to the right strip, moves windows between displays, and saves the layout.

use crate::ax::{self, Element};
use crate::controller::{focus_guard_armed, OverlayAction, OverlayEvent, StripController, Win};
use crate::events::WindowEvent;
use crate::geometry::{self, Rect};
use crate::input::Input;
use crate::layout_store::{self, SavedLayout};
use crate::overlays::Overview;
use crate::prefs::{self, OverviewMode, ScrollModifier};
use crate::shortcuts::{Combo, Command, Preset};
use objc2::MainThreadMarker;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

pub struct Desk {
    pub strips: Vec<StripController>,
    pub is_tiling: bool,
    pub swipe_available: bool,
    pub failed_shortcuts: Vec<Combo>,
    pub hotkeys_suspended: bool,
    saved: SavedLayout,
    pub preset: Preset,
    pub scroll_modifier: ScrollModifier,
    pub scroll_speed: f64,
    pub overview_mode: OverviewMode,
    pub show_border: bool,
    pub dim_inactive: bool,
    dragging: Option<String>,
    wheel_generation: u64,
    save_generation: u64,
    mtm: MainThreadMarker,
    pub last_tick: Instant,
}

impl Desk {
    pub fn new(mtm: MainThreadMarker) -> Desk {
        Desk {
            strips: vec![],
            is_tiling: false,
            swipe_available: false,
            failed_shortcuts: vec![],
            hotkeys_suspended: false,
            saved: layout_store::load(),
            preset: prefs::preset(),
            scroll_modifier: prefs::scroll_modifier(),
            scroll_speed: prefs::scroll_speed(),
            overview_mode: prefs::overview_mode(),
            show_border: prefs::bool("showFocusBorder"),
            dim_inactive: prefs::bool("dimInactive"),
            dragging: None,
            wheel_generation: 0,
            save_generation: 0,
            mtm,
            last_tick: Instant::now(),
        }
    }

    pub fn gap(&self) -> f64 {
        self.saved.gap
    }

    pub fn tiling_was_on(&self) -> bool {
        self.saved.tiling
    }

    // MARK: Tiling on / off

    pub fn start_tiling(&mut self) {
        if self.is_tiling {
            return;
        }
        self.is_tiling = true;
        crate::events::start(|e| crate::with_desk(move |d| d.handle(e)));
        self.build_strips();
        self.rescan_all();
        crate::input::set_scroll_flags(self.scroll_modifier.flags());
        self.swipe_available = !crate::test_mode::is_on() && crate::input::start_scroll_tap();
        crate::input::set_scroll_enabled(true);
        self.install_hotkeys();
        self.saved.tiling = true;
        self.save_now();
    }

    /// Stops tiling. `remember` keeps "tiling on" in the saved layout (used on quit).
    pub fn stop_tiling(&mut self, remember: bool) {
        if !self.is_tiling {
            return;
        }
        self.save_now(); // snapshot before strips are emptied
        self.is_tiling = false;
        crate::input::set_scroll_enabled(false);
        crate::input::uninstall();
        self.failed_shortcuts.clear();
        crate::events::stop();
        for s in &mut self.strips {
            s.stop();
        }
        self.strips.clear();
        self.saved.tiling = remember;
        layout_store::save(&self.saved);
    }

    pub fn set_gap(&mut self, gap: f64) {
        self.saved.gap = gap;
        for s in &mut self.strips {
            s.set_gap(gap);
        }
        self.schedule_save();
    }

    // MARK: Strips per display

    fn overlay_action() -> OverlayAction {
        Rc::new(|key: &str, e: OverlayEvent| {
            let key = key.to_string();
            crate::with_desk(move |d| {
                if let Some(s) = d.strips.iter_mut().find(|s| s.key() == key) {
                    match e {
                        OverlayEvent::Select { column, row } => s.select(column, row),
                        OverlayEvent::Tab { column, index } => s.select_tab(column, index),
                    }
                }
            })
        })
    }

    pub fn build_strips(&mut self) {
        let ph = geometry::primary_height(self.mtm);
        let displays = geometry::displays(self.mtm);
        let action = Self::overlay_action();
        self.strips = displays
            .iter()
            .map(|d| {
                let key = d.key.clone();
                let a = action.clone();
                let overview = Overview::new(self.mtm, Rc::new(move |column, row| a(&key, OverlayEvent::Select { column, row })));
                overview.mode.set(self.overview_mode);
                let mut s = StripController::new(d.clone(), self.saved.gap, ph, action.clone(), overview);
                s.show_border = self.show_border;
                s.dim_inactive = self.dim_inactive;
                // Which sides touch another display (parking must avoid them).
                let me = d.appkit_full;
                let overlaps = |r: &Rect| r.y < me.max_y() && r.max_y() > me.y;
                let left = displays.iter().any(|o| o.key != d.key && (o.appkit_full.max_x() - me.x).abs() < 2.0 && overlaps(&o.appkit_full));
                let right = displays.iter().any(|o| o.key != d.key && (o.appkit_full.x - me.max_x()).abs() < 2.0 && overlaps(&o.appkit_full));
                s.set_neighbors(left, right);
                s
            })
            .collect();
    }

    /// Every tileable window, with its pid and frame.
    pub fn tileable_windows() -> Vec<(Element, i32, Rect)> {
        let mut out = vec![];
        for app in crate::apps::managed() {
            let pid = app.processIdentifier();
            let app_el = Element::application(pid);
            app_el.set_timeout(0.5);
            for el in app_el.windows() {
                if !el.is_tileable() {
                    continue;
                }
                if let Some(f) = el.frame() {
                    out.push((el, pid, f));
                }
            }
        }
        out
    }

    /// Finds every tileable window and hands each strip the ones on its display. Windows
    /// already in a strip stay there (even while parked partly off screen).
    pub fn rescan_all(&mut self) {
        if !self.is_tiling {
            return;
        }
        let mut assigned: Vec<Vec<(Element, i32, Rect)>> = vec![vec![]; self.strips.len()];
        for (el, pid, frame) in Self::tileable_windows() {
            let owner = self.strips.iter().position(|s| s.contains(&el)).or_else(|| self.nearest(&frame));
            if let Some(i) = owner {
                assigned[i].push((el, pid, frame));
            }
        }
        for (s, live) in self.strips.iter_mut().zip(assigned) {
            let reference = self.saved.strips.get(s.key()).cloned();
            s.rebuild(live, reference.as_ref());
        }
    }

    /// The strip whose display contains the rect's centre, else the closest display.
    fn nearest(&self, frame: &Rect) -> Option<usize> {
        let (cx, cy) = (frame.mid_x(), frame.mid_y());
        let dist = |r: &Rect| {
            let dx = (r.x - cx).max(0.0).max(cx - r.max_x());
            let dy = (r.y - cy).max(0.0).max(cy - r.max_y());
            dx * dx + dy * dy
        };
        (0..self.strips.len()).min_by(|&a, &b| dist(&self.strips[a].display.full).total_cmp(&dist(&self.strips[b].display.full)))
    }

    /// Displays came or went: rebuild from the just-saved layout.
    pub fn screens_changed(&mut self) {
        if !self.is_tiling {
            return;
        }
        self.save_now();
        for s in &mut self.strips {
            s.stop();
        }
        self.build_strips();
        self.rescan_all();
    }

    /// The strip holding the focused window; else the one under the pointer.
    fn active_strip(&self) -> Option<usize> {
        if let Some(el) = Element::system_focused_window() {
            if let Some(i) = self.strips.iter().position(|s| s.contains(&el)) {
                return Some(i);
            }
        }
        self.strip_at(crate::mouse_location_ax(self.mtm))
    }

    fn strip_at(&self, (x, y): (f64, f64)) -> Option<usize> {
        self.strips.iter().position(|s| s.display.full.contains_point(x, y)).or(if self.strips.is_empty() { None } else { Some(0) })
    }

    fn by_position(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.strips.len()).collect();
        v.sort_by(|&a, &b| self.strips[a].display.full.x.total_cmp(&self.strips[b].display.full.x));
        v
    }

    fn focus_display(&mut self, delta: isize) {
        let order = self.by_position();
        let Some(cur) = self.active_strip() else { return };
        let Some(i) = order.iter().position(|&s| s == cur) else { return };
        if let Some(&t) = order.get((i as isize + delta) as usize) {
            self.strips[t].activate();
        }
    }

    fn move_window_to_display(&mut self, delta: isize) {
        let order = self.by_position();
        let Some(cur) = self.active_strip() else { return };
        let Some(i) = order.iter().position(|&s| s == cur) else { return };
        let Some(&t) = order.get((i as isize + delta) as usize) else {
            crate::controller::beep();
            return;
        };
        if let Some(w) = self.strips[cur].take_focused_window() {
            self.strips[t].receive(w);
        }
    }

    // MARK: Events

    pub fn handle(&mut self, e: WindowEvent) {
        if !self.is_tiling {
            return;
        }
        match e {
            WindowEvent::Created(el, pid) | WindowEvent::Restored(el, pid) => self.adopt(el, pid, 4),
            WindowEvent::Destroyed(el) | WindowEvent::Minimized(el) => {
                for s in &mut self.strips {
                    if s.remove(&el) {
                        break;
                    }
                }
            }
            WindowEvent::Resized(el) => {
                if let Some(s) = self.strips.iter_mut().find(|s| s.contains(&el)) {
                    s.window_resized(&el);
                }
            }
            WindowEvent::FocusChanged { pid, window } => self.follow_focus(pid, window, 4),
            WindowEvent::AppQuit(pid) => {
                crate::with_mover(|m| m.forget(pid));
                for s in &mut self.strips {
                    s.remove_app(pid);
                }
            }
        }
    }

    /// Something outside Reel brought a window forward (Spotlight, Dock, ⌘Tab, Mission
    /// Control, a click...). The system-wide focused window can still name the previous app
    /// right after an activation, so ask the activated app itself, and retry briefly.
    fn follow_focus(&mut self, pid: i32, window: Option<Element>, attempts: u32) {
        // Activation notices arrive for every app, including ones Reel must leave alone
        // (menu-bar apps, the test host, everything except the test host in test mode).
        if focus_guard_armed() || !crate::apps::manages_pid(pid) {
            return;
        }
        let app = Element::application(pid);
        let el = window.or_else(|| app.element(ax::FOCUSED_WINDOW)).or_else(|| app.element(ax::MAIN_WINDOW));
        if let Some(el) = &el {
            if let Some(s) = self.strips.iter_mut().find(|s| s.contains(el)) {
                s.follow_system_focus(el);
                return;
            }
        }
        if attempts > 0 {
            crate::later(0.08, move |d| d.follow_focus(pid, None, attempts - 1));
        } else if let Some(el) = el {
            // A window we never saw created: adopt it now that the user is using it.
            self.adopt(el, pid, 0);
        }
    }

    /// New windows can arrive before their subrole/frame is readable; retry briefly.
    fn adopt(&mut self, el: Element, pid: i32, attempts: u32) {
        if self.strips.iter().any(|s| s.contains(&el)) || !crate::apps::manages_pid(pid) {
            return;
        }
        let frame = if el.is_tileable() { el.frame() } else { None };
        let Some(frame) = frame else {
            if attempts > 0 {
                crate::later(0.12, move |d| d.adopt(el, pid, attempts - 1));
            }
            return;
        };
        if let Some(i) = self.nearest(&frame) {
            self.strips[i].receive(Win::new(el, pid));
        }
    }

    // MARK: Input

    pub fn input(&mut self, i: Input) {
        match i {
            Input::Shortcut(c) => {
                crate::log::write(&format!("key {} ({})", self.bindings().get(&c).map(|k| k.glyphs()).unwrap_or_default(), c.raw()));
                crate::close_menu();
                self.perform(c);
            }
            Input::ScrollBegin { x, y } => self.begin_drag(x, y),
            Input::ScrollDelta(dx) => self.drag(dx),
            Input::ScrollEnd(v) => self.end_drag(v),
            Input::WheelDelta { x, y, delta } => {
                if self.dragging.is_none() {
                    self.begin_drag(x, y);
                }
                self.drag(delta);
                self.wheel_generation += 1;
                let generation = self.wheel_generation;
                crate::later(0.18, move |d| {
                    if d.wheel_generation == generation {
                        d.end_drag(0.0);
                    }
                });
            }
        }
    }

    fn begin_drag(&mut self, x: f64, y: f64) {
        let Some(i) = self.strips.iter().position(|s| s.display.full.contains_point(x, y)) else { return };
        if self.strips[i].strip.columns.is_empty() {
            return;
        }
        self.strips[i].begin_drag();
        self.dragging = Some(self.strips[i].key().to_string());
        crate::ensure_clock();
    }

    fn drag(&mut self, dx: f64) {
        let speed = self.scroll_speed;
        if let Some(s) = self.dragging_strip() {
            s.drag(dx * speed);
            crate::ensure_clock();
        }
    }

    fn end_drag(&mut self, v: f64) {
        let speed = self.scroll_speed;
        if let Some(s) = self.dragging_strip() {
            s.end_drag(v * speed);
        }
        self.dragging = None;
        crate::ensure_clock();
    }

    fn dragging_strip(&mut self) -> Option<&mut StripController> {
        let key = self.dragging.clone()?;
        self.strips.iter_mut().find(|s| s.key() == key)
    }

    pub fn perform(&mut self, c: Command) {
        match c {
            Command::FocusDisplayLeft => return self.focus_display(-1),
            Command::FocusDisplayRight => return self.focus_display(1),
            Command::SendToDisplayLeft => return self.move_window_to_display(-1),
            Command::SendToDisplayRight => return self.move_window_to_display(1),
            _ => {}
        }
        if !self.is_tiling {
            return crate::log::write("  ignored: tiling is off");
        }
        let Some(i) = self.active_strip() else { return crate::log::write("  ignored: no strip") };
        let s = &mut self.strips[i];
        use Command::*;
        match c {
            FocusLeft => s.focus_column(-1),
            FocusRight => s.focus_column(1),
            FocusUp => s.focus_in_stack(-1),
            FocusDown => s.focus_in_stack(1),
            MoveLeft => s.move_column(-1),
            MoveRight => s.move_column(1),
            MoveUp => s.move_in_stack(-1),
            MoveDown => s.move_in_stack(1),
            StackLeft => s.stack(-1),
            StackRight => s.stack(1),
            PopOutLeft => s.expel(-1),
            PopOutRight => s.expel(1),
            ToggleTabbed => s.toggle_tabbed(),
            UnstackAll => s.unstack_all(),
            WidthNext => s.cycle_width(true),
            WidthPrevious => s.cycle_width(false),
            Narrower => s.adjust_width(-1),
            Wider => s.adjust_width(1),
            FullWidth => s.toggle_full_width(),
            Center => s.center(),
            LastColumn => s.focus_last_column(),
            _ => {
                if let Some(n) = c.column_number() {
                    s.focus_column_at(n - 1);
                }
            }
        }
        crate::ensure_clock();
    }

    pub fn unstack_all(&mut self) {
        for s in &mut self.strips {
            s.unstack_all();
        }
        crate::ensure_clock();
    }

    // MARK: Animation

    pub fn any_animating(&self) -> bool {
        self.strips.iter().any(|s| s.animating)
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        // Cap dt so a stall doesn't make springs jump.
        let dt = now.duration_since(self.last_tick).as_secs_f64().min(1.0 / 30.0);
        self.last_tick = now;
        let mut any = false;
        for s in &mut self.strips {
            if s.animating {
                any |= s.tick(dt);
            }
        }
        any
    }

    // MARK: Shortcuts

    pub fn bindings(&self) -> HashMap<Command, Combo> {
        match self.preset {
            Preset::Custom => prefs::custom_bindings().unwrap_or_else(|| Preset::Shift.bindings()),
            p => p.bindings(),
        }
    }

    pub fn set_preset(&mut self, p: Preset) {
        // First time customizing: start from whatever was active, not from nothing.
        if p == Preset::Custom && prefs::custom_bindings().is_none() {
            prefs::set_custom_bindings(Some(&self.bindings()));
        }
        self.preset = p;
        prefs::set_preset(p);
        self.reinstall_hotkeys();
    }

    /// Sets (or clears) a custom binding. A combo can only do one thing: any other command
    /// using it loses it, and that command is returned so the UI can say so.
    pub fn set_custom_binding(&mut self, c: Command, combo: Option<Combo>) -> Option<Command> {
        let mut map = if self.preset == Preset::Custom { prefs::custom_bindings().unwrap_or_else(|| self.bindings()) } else { self.bindings() };
        let mut displaced = None;
        if let Some(k) = combo {
            if let Some((&other, _)) = map.iter().find(|(o, v)| **o != c && **v == k) {
                map.remove(&other);
                displaced = Some(other);
            }
            map.insert(c, k);
        } else {
            map.remove(&c);
        }
        prefs::set_custom_bindings(Some(&map));
        self.preset = Preset::Custom;
        prefs::set_preset(Preset::Custom);
        self.reinstall_hotkeys();
        displaced
    }

    pub fn reset_custom_bindings(&mut self, to: Preset) {
        prefs::set_custom_bindings(Some(&to.bindings()));
        self.reinstall_hotkeys();
    }

    pub fn set_hotkeys_suspended(&mut self, on: bool) {
        self.hotkeys_suspended = on;
        self.reinstall_hotkeys();
    }

    fn reinstall_hotkeys(&mut self) {
        if self.is_tiling && !self.hotkeys_suspended {
            self.install_hotkeys();
        } else {
            crate::input::uninstall();
            self.failed_shortcuts.clear();
        }
    }

    /// Registers the active keymap (only while tiling, so keys like ⇧← keep their normal
    /// meaning the rest of the time).
    fn install_hotkeys(&mut self) {
        if crate::test_mode::is_on() || self.hotkeys_suspended {
            return;
        }
        self.failed_shortcuts = crate::input::install(&self.bindings());
        crate::log::write(&if self.failed_shortcuts.is_empty() {
            format!("all shortcuts registered ({})", self.preset.label())
        } else {
            format!("shortcuts NOT registered: {}", self.failed_shortcuts.iter().map(Combo::glyphs).collect::<Vec<_>>().join(" "))
        });
    }

    // MARK: Settings

    pub fn set_scroll_modifier(&mut self, m: ScrollModifier) {
        self.scroll_modifier = m;
        prefs::set_string("scrollModifier", m.raw());
        crate::input::set_scroll_flags(m.flags());
    }

    pub fn set_scroll_speed(&mut self, v: f64) {
        self.scroll_speed = v;
        prefs::set_double("scrollSpeed", v);
    }

    pub fn set_overview_mode(&mut self, m: OverviewMode) {
        self.overview_mode = m;
        prefs::set_string("overviewMode", m.raw());
        for s in &self.strips {
            s.overview.mode.set(m);
            s.overview.apply_mode();
        }
    }

    pub fn set_decor(&mut self, border: bool, dim: bool) {
        self.show_border = border;
        self.dim_inactive = dim;
        prefs::set_bool("showFocusBorder", border);
        prefs::set_bool("dimInactive", dim);
        for s in &mut self.strips {
            s.show_border = border;
            s.dim_inactive = dim;
            s.clear_decor();
            s.commit(false, false);
        }
    }

    // MARK: Saving

    pub fn schedule_save(&mut self) {
        self.save_generation += 1;
        let generation = self.save_generation;
        crate::later(1.0, move |d| {
            if d.save_generation == generation {
                d.save_now();
            }
        });
    }

    /// Writes the current strips. Layouts for displays that aren't connected are kept.
    pub fn save_now(&mut self) {
        if !self.is_tiling {
            return;
        }
        for s in &self.strips {
            self.saved.strips.insert(s.key().to_string(), s.snapshot());
        }
        layout_store::save(&self.saved);
    }

    // MARK: Recovery

    /// Moves every window that is (almost) entirely off screen back into view: windows a
    /// previous Reel left parked at the edges if it was force-quit. Returns how many moved.
    pub fn rescue_offscreen_windows(&self) -> usize {
        let displays = geometry::displays(self.mtm);
        let Some(home) = displays.first().map(|d| d.usable) else { return 0 };
        let mut moved = 0;
        for app in crate::apps::managed() {
            let app_el = Element::application(app.processIdentifier());
            app_el.set_timeout(0.5);
            for el in app_el.windows() {
                if el.subrole().as_deref() != Some(ax::STANDARD_WINDOW) || el.is_minimized() {
                    continue;
                }
                let Some(f) = el.frame() else { continue };
                let shown = displays.iter().map(|d| d.usable.overlap_area(&f)).fold(0.0, f64::max);
                if shown >= 100.0 * 100.0 {
                    continue;
                }
                let step = (moved % 8) as f64 * 28.0;
                el.set_position(home.x + 60.0 + step, home.y + 40.0 + step);
                moved += 1;
            }
        }
        crate::log::write(&format!("rescued {moved} off-screen window(s)"));
        moved
    }
}
