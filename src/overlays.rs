//! Reel's own on-screen pieces, all borderless non-activating panels drawn with Core
//! Animation layers: the overview pill, the focus border and dimming veils, and the tab
//! bar above tabbed columns. (macOS has no public way to draw on another app's window.)

use crate::geometry::{to_appkit, Rect};
use crate::objc_classes::{HudView, Target};
use crate::prefs::OverviewMode;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPanel, NSSegmentSwitchTracking, NSSegmentedControl, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{kCACornerCurveContinuous, CALayer, CATransaction};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const STATUS_BAR_LEVEL: isize = 25;
const FLOATING_LEVEL: isize = 3;

fn ns_rect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}

fn panel(mtm: MainThreadMarker, level: isize, click_through: bool) -> Retained<NSPanel> {
    let p = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        NSRect::ZERO,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        true,
    );
    unsafe { p.setReleasedWhenClosed(false) };
    p.setLevel(level);
    p.setOpaque(false);
    p.setBackgroundColor(Some(&NSColor::clearColor()));
    p.setHasShadow(false);
    p.setIgnoresMouseEvents(click_through);
    p.setHidesOnDeactivate(false);
    p.setCollectionBehavior(
        NSWindowCollectionBehavior::IgnoresCycle | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::Transient,
    );
    p
}

fn layer_view(mtm: MainThreadMarker) -> (Retained<NSView>, Retained<CALayer>) {
    let view = NSView::new(mtm);
    let layer = CALayer::layer();
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    (view, layer)
}

fn color(white: f64, alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithWhite_alpha(white, alpha)
}

fn no_animation(f: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    f();
    CATransaction::commit();
}

// MARK: Overview

/// What the overview draws: a scaled-down picture of one strip.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub tiles: Vec<Tile>,
    pub total: f64,
    pub viewport: (f64, f64),
}

#[derive(Clone)]
pub struct Tile {
    pub x: f64,
    pub width: f64,
    pub rows: usize,
    pub tabbed: bool,
    /// Set only on the focused column.
    pub focused_row: Option<usize>,
}

struct OverviewState {
    snapshot: Snapshot,
    scale: f64,
    hovering: bool,
    hide_generation: u64,
}

/// A compact minimap pill above the Dock: one block per column (split for stacks, a tab
/// marker for tabbed columns), the focused window in the accent colour, and an outline
/// around the part that's on screen. Click a block to jump there; hovering keeps it up.
pub struct Overview {
    panel: Retained<NSPanel>,
    root: Retained<CALayer>,
    viewport_layer: Retained<CALayer>,
    tiles: RefCell<Vec<Retained<CALayer>>>,
    state: Rc<RefCell<OverviewState>>,
    usable_appkit: Cell<Rect>,
    pub mode: Cell<OverviewMode>,
    mtm: MainThreadMarker,
}

const MAP_HEIGHT: f64 = 30.0;
const PAD: f64 = 7.0;
const TILE_GAP: f64 = 2.0;

impl Overview {
    /// `on_select(column, row)` runs when a block is clicked.
    pub fn new(mtm: MainThreadMarker, on_select: Rc<dyn Fn(usize, usize)>) -> Rc<Overview> {
        let panel = panel(mtm, STATUS_BAR_LEVEL, false);
        panel.setHasShadow(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        let root = CALayer::layer();
        let state = Rc::new(RefCell::new(OverviewState { snapshot: Snapshot::default(), scale: 0.03, hovering: false, hide_generation: 0 }));
        let ov: Rc<Overview> = Rc::new_cyclic(|weak: &std::rc::Weak<Overview>| {
            let (w1, w2) = (weak.clone(), weak.clone());
            let select = on_select.clone();
            let view = HudView::new(
                mtm,
                move |p| {
                    if let Some(o) = w1.upgrade() {
                        if let Some((c, r)) = o.hit(p.x, p.y) {
                            select(c, r);
                        }
                    }
                },
                move |inside| {
                    if let Some(o) = w2.upgrade() {
                        o.state.borrow_mut().hovering = inside;
                        if inside {
                            o.state.borrow_mut().hide_generation += 1;
                        } else {
                            o.schedule_hide();
                        }
                    }
                },
            );
            view.setLayer(Some(&root));
            view.setWantsLayer(true);
            panel.setContentView(Some(&view));
            let viewport_layer = CALayer::layer();
            Overview {
                panel,
                root,
                viewport_layer,
                tiles: RefCell::new(vec![]),
                state,
                usable_appkit: Cell::new(Rect::default()),
                mode: Cell::new(OverviewMode::Auto),
                mtm,
            }
        });
        ov.root.setBackgroundColor(Some(&color(0.13, 0.88).CGColor()));
        ov.root.setCornerRadius(11.0);
        ov.root.setCornerCurve(unsafe { kCACornerCurveContinuous });
        ov.root.setBorderColor(Some(&color(1.0, 0.08).CGColor()));
        ov.root.setBorderWidth(0.5);
        ov.viewport_layer.setBorderColor(Some(&color(1.0, 0.92).CGColor()));
        ov.viewport_layer.setBorderWidth(1.5);
        ov.viewport_layer.setCornerRadius(5.0);
        ov.viewport_layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
        ov.viewport_layer.setZPosition(10.0);
        ov.root.addSublayer(&ov.viewport_layer);
        ov
    }

    pub fn set_usable(&self, appkit_usable: Rect) {
        self.usable_appkit.set(appkit_usable);
    }

    /// Redraws tiles for a new arrangement; shows the overview unless `show` is false.
    pub fn update(&self, snap: Snapshot, offset: f64, show: bool) {
        let span = snap.total.max(snap.viewport.0);
        let scale = (MAP_HEIGHT / snap.viewport.1.max(1.0)).min(360.0 / span.max(1.0));
        let map_w = (span * scale).round();
        let map_h = (snap.viewport.1 * scale).round();
        let (w, h) = (map_w + 2.0 * PAD, map_h + 2.0 * PAD);
        let u = self.usable_appkit.get();
        self.panel.setFrame_display(ns_rect(Rect::new((u.mid_x() - w / 2.0).round(), u.y + 14.0, w, h)), false);

        let accent = NSColor::controlAccentColor().CGColor();
        let idle = color(1.0, 0.30).CGColor();
        no_animation(|| {
            for l in self.tiles.borrow_mut().drain(..) {
                l.removeFromSuperlayer();
            }
            let mut tiles = self.tiles.borrow_mut();
            for tile in &snap.tiles {
                let x = PAD + tile.x * scale;
                let tw = (tile.width * scale - TILE_GAP).max(3.0);
                let n = if tile.tabbed { 1 } else { tile.rows.max(1) };
                let cell_h = (map_h - (n as f64 - 1.0) * TILE_GAP) / n as f64;
                for r in 0..n {
                    let cell = CALayer::layer();
                    let y = PAD + map_h - (r as f64 + 1.0) * cell_h - r as f64 * TILE_GAP;
                    cell.setFrame(ns_rect(Rect::new(x, y, tw, cell_h)));
                    cell.setCornerRadius(2.5);
                    let focused = if tile.tabbed { tile.focused_row.is_some() } else { tile.focused_row == Some(r) };
                    cell.setBackgroundColor(Some(if focused { &accent } else { &idle }));
                    self.root.addSublayer(&cell);
                    tiles.push(cell);
                }
                if tile.tabbed && tile.rows > 1 {
                    let seg_w = (tw - (tile.rows as f64 - 1.0)) / tile.rows as f64;
                    for t in 0..tile.rows {
                        let seg = CALayer::layer();
                        seg.setFrame(ns_rect(Rect::new(x + t as f64 * (seg_w + 1.0), PAD + map_h - 2.5, seg_w, 2.5)));
                        let c = if tile.focused_row == Some(t) { color(1.0, 1.0) } else { color(0.0, 0.35) };
                        seg.setBackgroundColor(Some(&c.CGColor()));
                        self.root.addSublayer(&seg);
                        tiles.push(seg);
                    }
                }
            }
        });
        {
            let mut st = self.state.borrow_mut();
            st.snapshot = snap;
            st.scale = scale;
        }
        self.set_offset(offset);
        if show || self.mode.get() == OverviewMode::Always {
            self.show();
        }
    }

    /// Moves the "on screen" outline. Cheap enough to call every frame.
    pub fn set_offset(&self, offset: f64) {
        let st = self.state.borrow();
        let map_h = (st.snapshot.viewport.1 * st.scale).round();
        let r = Rect::new(PAD + offset * st.scale - 2.0, PAD - 2.0, st.snapshot.viewport.0 * st.scale + 4.0 - TILE_GAP, map_h + 4.0);
        no_animation(|| self.viewport_layer.setFrame(ns_rect(r)));
    }

    pub fn show(&self) {
        if self.mode.get() == OverviewMode::Off || self.state.borrow().snapshot.tiles.is_empty() {
            self.hide();
            return;
        }
        self.panel.setAlphaValue(1.0);
        if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
        self.schedule_hide();
    }

    fn schedule_hide(&self) {
        let generation = {
            let mut st = self.state.borrow_mut();
            st.hide_generation += 1;
            st.hide_generation
        };
        if self.mode.get() != OverviewMode::Auto || self.state.borrow().hovering {
            return;
        }
        let panel = self.panel.clone();
        let state = self.state.clone();
        crate::ffi::main_after(1.2, move || {
            if state.borrow().hide_generation == generation && !state.borrow().hovering {
                panel.orderOut(None);
            }
        });
    }

    pub fn hide(&self) {
        self.state.borrow_mut().hide_generation += 1;
        self.panel.orderOut(None);
    }

    pub fn apply_mode(&self) {
        match self.mode.get() {
            OverviewMode::Off => self.hide(),
            OverviewMode::Always => self.show(),
            OverviewMode::Auto => self.schedule_hide(),
        }
    }

    /// Which (column, row) is under a click at view coordinates.
    fn hit(&self, px: f64, py: f64) -> Option<(usize, usize)> {
        let st = self.state.borrow();
        let map_h = (st.snapshot.viewport.1 * st.scale).round();
        for (i, t) in st.snapshot.tiles.iter().enumerate() {
            let x = PAD + t.x * st.scale;
            if px < x || px > x + (t.width * st.scale).max(3.0) {
                continue;
            }
            let row = if t.tabbed && t.rows > 1 {
                (t.focused_row.map(|r| r + 1).unwrap_or(0)) % t.rows // click cycles tabs
            } else if t.rows > 1 {
                let from_top = PAD + map_h - py;
                ((from_top / (map_h / t.rows as f64)) as usize).min(t.rows - 1)
            } else {
                0
            };
            return Some((i, row));
        }
        None
    }

    pub fn close(&self) {
        self.panel.orderOut(None);
    }

    pub fn mtm(&self) -> MainThreadMarker {
        self.mtm
    }
}

// MARK: Focus border and dimming

/// One click-through panel showing a border ring or a dark veil.
pub struct Ring {
    panel: Retained<NSPanel>,
    last: Cell<Option<Rect>>,
}

impl Ring {
    pub fn new(mtm: MainThreadMarker, border: bool) -> Ring {
        let panel = panel(mtm, FLOATING_LEVEL, true);
        let (view, layer) = layer_view(mtm);
        layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
        if border {
            layer.setCornerRadius(13.0);
            layer.setBorderWidth(3.0);
            layer.setBorderColor(Some(&NSColor::controlAccentColor().CGColor()));
        } else {
            layer.setCornerRadius(10.0);
            layer.setBackgroundColor(Some(&color(0.0, 0.28).CGColor()));
        }
        panel.setContentView(Some(&view));
        Ring { panel, last: Cell::new(None) }
    }

    /// Places it over an AX-coordinate rect.
    pub fn place(&self, ax: Rect, primary_height: f64) {
        let r = to_appkit(ax, primary_height);
        let r = Rect::new(r.x.round(), r.y.round(), r.w.round(), r.h.round());
        if self.last.get() != Some(r) {
            self.panel.setFrame_display(ns_rect(r), false);
            self.last.set(Some(r));
        }
        if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
    }

    pub fn hide(&self) {
        self.panel.orderOut(None);
        self.last.set(None);
    }
}

impl Drop for Ring {
    fn drop(&mut self) {
        self.panel.orderOut(None);
    }
}

// MARK: Tab bar

/// The row of tabs above a tabbed column: a native segmented control in a panel.
pub struct TabBar {
    panel: Retained<NSPanel>,
    control: RefCell<Option<Retained<NSSegmentedControl>>>,
    target: Retained<Target>,
    titles: RefCell<Vec<String>>,
    mtm: MainThreadMarker,
}

impl TabBar {
    /// `on_select(index)` runs when a tab is clicked.
    pub fn new(mtm: MainThreadMarker, on_select: impl Fn(usize) + 'static) -> TabBar {
        let panel = panel(mtm, FLOATING_LEVEL, false);
        panel.setBecomesKeyOnlyIfNeeded(true);
        let target = Target::new(mtm, move |sender: &AnyObject| {
            if let Some(c) = sender.downcast_ref::<NSSegmentedControl>() {
                on_select(c.selectedSegment().max(0) as usize);
            }
        });
        TabBar { panel, control: RefCell::new(None), target, titles: RefCell::new(vec![]), mtm }
    }

    pub fn update(&self, titles: &[String], active: usize) {
        if *self.titles.borrow() != titles || self.control.borrow().is_none() {
            let labels: Vec<Retained<NSString>> =
                titles.iter().map(|t| NSString::from_str(if t.is_empty() { "(untitled)" } else { t })).collect();
            let array = NSArray::from_retained_slice(&labels);
            let control = unsafe {
                NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                    &array,
                    NSSegmentSwitchTracking::SelectOne,
                    Some(&self.target),
                    Some(Target::action()),
                    self.mtm,
                )
            };
            self.panel.setContentView(Some(&control));
            *self.control.borrow_mut() = Some(control);
            *self.titles.borrow_mut() = titles.to_vec();
        }
        if let Some(c) = self.control.borrow().as_ref() {
            c.setSelectedSegment(active as isize);
        }
    }

    /// Places the bar at an AX rect; hides it when nothing of it is on screen.
    pub fn place(&self, ax: Option<Rect>, usable: Rect, primary_height: f64) {
        match ax {
            Some(r) if r.overlap_area(&usable) > 0.0 => {
                let a = to_appkit(r, primary_height);
                self.panel.setFrame_display(ns_rect(a), false);
                if let Some(c) = self.control.borrow().as_ref() {
                    for i in 0..c.segmentCount() {
                        c.setWidth_forSegment((a.w - 8.0) / c.segmentCount() as f64, i);
                    }
                }
                if !self.panel.isVisible() {
                    self.panel.orderFrontRegardless();
                }
            }
            _ => self.panel.orderOut(None),
        }
    }
}

impl Drop for TabBar {
    fn drop(&mut self) {
        self.panel.orderOut(None);
    }
}
