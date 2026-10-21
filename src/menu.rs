//! The menu bar item: a standard macOS dropdown menu, rebuilt each time it opens.

use crate::objc_classes::{MenuDelegate, Target};
use crate::prefs::{OverviewMode, ScrollModifier};
use crate::shortcuts::{Command, Preset, MODIFIER_LEGEND};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::MainThreadOnly;
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
use objc2_foundation::{MainThreadMarker, NSString};
use std::cell::RefCell;

thread_local! {
    static MENU: RefCell<Option<MenuState>> = const { RefCell::new(None) };
}

struct MenuState {
    _item: Retained<NSStatusItem>,
    menu: Retained<NSMenu>,
    _delegate: Retained<MenuDelegate>,
    /// Menu items hold their targets weakly; keep this rebuild's targets alive.
    targets: Vec<Retained<Target>>,
}

pub fn install(mtm: MainThreadMarker) {
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(-2.0); // square
    if let Some(button) = item.button(mtm) {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("rectangle.split.3x1"),
            Some(&NSString::from_str("Reel")),
        );
        button.setImage(image.as_deref());
    }
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let delegate = MenuDelegate::new(mtm, move |m: &NSMenu| rebuild(m, mtm));
    menu.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    item.setMenu(Some(&menu));
    MENU.with(|s| *s.borrow_mut() = Some(MenuState { _item: item, menu, _delegate: delegate, targets: vec![] }));
}

/// Closes the menu if it's open (a shortcut fired while it was showing).
pub fn close() {
    MENU.with(|s| {
        if let Some(st) = s.borrow().as_ref() {
            st.menu.cancelTracking();
        }
    });
}

/// Builds menu items, collecting their targets.
struct Builder<'a> {
    mtm: MainThreadMarker,
    targets: &'a mut Vec<Retained<Target>>,
}

impl Builder<'_> {
    fn item(&mut self, menu: &NSMenu, title: &str, checked: bool, action: Option<Box<dyn Fn()>>) -> Retained<NSMenuItem> {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm), &NSString::from_str(title), None, &NSString::from_str(""))
        };
        if checked {
            item.setState(1);
        }
        match action {
            Some(f) => {
                let target = Target::new(self.mtm, move |_: &AnyObject| f());
                unsafe {
                    item.setTarget(Some(&target));
                    item.setAction(Some(Target::action()));
                }
                self.targets.push(target);
            }
            None => item.setEnabled(false),
        }
        menu.addItem(&item);
        item
    }

    fn action(&mut self, menu: &NSMenu, title: &str, checked: bool, f: impl Fn() + 'static) -> Retained<NSMenuItem> {
        self.item(menu, title, checked, Some(Box::new(f)))
    }

    fn label(&mut self, menu: &NSMenu, title: &str) {
        self.item(menu, title, false, None);
    }

    fn submenu(&mut self, menu: &NSMenu, title: &str) -> Retained<NSMenu> {
        let sub = NSMenu::new(self.mtm);
        sub.setAutoenablesItems(false);
        let item = self.item(menu, title, false, Some(Box::new(|| {})));
        item.setSubmenu(Some(&sub));
        sub
    }
}

fn sep(menu: &NSMenu, mtm: MainThreadMarker) {
    menu.addItem(&NSMenuItem::separatorItem(mtm));
}

/// One column line in the menu: label, width/stack detail, focused.
type ColumnLine = (String, String, bool);

/// What the menu needs to know, read in one go.
struct View {
    trusted: bool,
    tiling: bool,
    swipe: bool,
    strips: Vec<(String, Vec<ColumnLine>)>,
    has_stacks: bool,
    preset: Preset,
    bindings: std::collections::HashMap<Command, crate::shortcuts::Combo>,
    failed: Vec<String>,
    scroll_modifier: ScrollModifier,
    scroll_speed: f64,
    overview: OverviewMode,
    border: bool,
    dim: bool,
    gap: f64,
}

fn read_view() -> Option<View> {
    crate::with_desk_sync(|d| {
        let multi = d.strips.len() > 1;
        View {
            trusted: crate::ax::is_trusted(),
            tiling: d.is_tiling,
            swipe: d.swipe_available,
            strips: d
                .strips
                .iter()
                .map(|s| {
                    let name = if multi { s.display.name.clone() } else { "Columns".into() };
                    let rows = s
                        .strip
                        .columns
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let w = c.focused_window().and_then(|id| s.wins.get(&id));
                            let label = w.map(|w| {
                                let t = w.title();
                                if t.is_empty() { w.app.clone() } else { format!("{} — {}", w.app, t) }
                            });
                            let mut detail = format!("{}%", (c.share * 100.0).round());
                            if c.windows.len() > 1 {
                                detail += &if c.tabbed { format!(", {} tabs", c.windows.len()) } else { format!(", stack of {}", c.windows.len()) };
                            }
                            (label.unwrap_or_else(|| "?".into()), detail, i == s.strip.focus)
                        })
                        .collect();
                    (name, rows)
                })
                .collect(),
            has_stacks: d.strips.iter().any(|s| s.strip.columns.iter().any(|c| c.windows.len() > 1)),
            preset: d.preset,
            bindings: d.bindings(),
            failed: d.failed_shortcuts.iter().map(|c| c.glyphs()).collect(),
            scroll_modifier: d.scroll_modifier,
            scroll_speed: d.scroll_speed,
            overview: d.overview_mode,
            border: d.show_border,
            dim: d.dim_inactive,
            gap: d.gap(),
        }
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}

fn rebuild(menu: &NSMenu, mtm: MainThreadMarker) {
    menu.removeAllItems();
    let mut targets = vec![];
    let mut b = Builder { mtm, targets: &mut targets };
    let Some(v) = read_view() else { return };

    if !v.trusted {
        b.label(menu, "Reel needs Accessibility access");
        b.action(menu, "Open Accessibility Settings…", false, crate::open_accessibility_settings);
        sep(menu, mtm);
        b.action(menu, "Quit Reel", false, crate::quit);
        MENU.with(|s| s.borrow_mut().as_mut().map(|st| st.targets = targets));
        return;
    }

    b.action(menu, "Tile Windows", v.tiling, move || crate::set_tiling(!v.tiling));
    sep(menu, mtm);

    if v.tiling {
        for (si, (name, rows)) in v.strips.iter().enumerate() {
            b.label(menu, &format!("{name} · {} columns", rows.len()));
            for (ci, (label, detail, focused)) in rows.iter().enumerate() {
                let title = format!("{}   {}", truncate(label, 48), detail);
                b.action(menu, &title, *focused, move || {
                    crate::with_desk(move |d| {
                        if let Some(s) = d.strips.get_mut(si) {
                            s.select(ci, s.strip.columns.get(ci).map(|c| c.row).unwrap_or(0));
                        }
                        crate::ensure_clock();
                    })
                });
            }
        }
        if v.has_stacks {
            b.action(menu, "Unstack All", false, || crate::with_desk(|d| d.unstack_all()));
        }
        sep(menu, mtm);
    }

    // Shortcuts.
    let shortcuts = b.submenu(menu, &format!("Shortcuts: {}", v.preset.label()));
    for p in Preset::ALL {
        b.action(&shortcuts, p.label(), v.preset == p, move || {
            crate::with_desk(move |d| d.set_preset(p));
            if p == Preset::Custom {
                crate::editor::show();
            }
        });
    }
    b.action(&shortcuts, "Edit Shortcuts…", false, || {
        crate::with_desk(|d| {
            if d.preset != Preset::Custom {
                d.set_preset(Preset::Custom);
            }
        });
        crate::editor::show();
    });
    if v.preset == Preset::Shift {
        b.label(&shortcuts, "While tiling, ⇧ Shift + arrows won't select text in apps.");
    }
    sep(&shortcuts, mtm);
    b.label(&shortcuts, MODIFIER_LEGEND);
    for (group, commands) in Command::groups() {
        sep(&shortcuts, mtm);
        b.label(&shortcuts, group);
        for c in commands {
            let keys = v.bindings.get(&c).map(|k| k.spelled()).unwrap_or_else(|| "none".into());
            b.label(&shortcuts, &format!("{}  —  {}", c.title(), keys));
        }
    }
    sep(&shortcuts, mtm);
    b.label(&shortcuts, &format!("Scroll the strip  —  {} + two-finger swipe or wheel", v.scroll_modifier.spelled()));
    if !v.failed.is_empty() {
        b.label(menu, &format!("⚠︎ Shortcuts unavailable: {}", v.failed.join(" ")));
    }

    // Scrolling.
    let scroll = b.submenu(menu, &format!("Scroll With: {}", v.scroll_modifier.spelled()));
    for m in ScrollModifier::ALL {
        b.action(&scroll, &format!("{} + scroll", m.spelled()), v.scroll_modifier == m, move || crate::with_desk(move |d| d.set_scroll_modifier(m)));
    }
    if let Some(c) = v.scroll_modifier.caveat() {
        sep(&scroll, mtm);
        b.label(&scroll, c);
    }
    let speed = b.submenu(menu, &format!("Scroll Speed: {}×", v.scroll_speed));
    for s in [1.0, 1.5, 2.0, 3.0, 4.0] {
        b.action(&speed, &format!("{s}×"), (v.scroll_speed - s).abs() < 0.01, move || crate::with_desk(move |d| d.set_scroll_speed(s)));
    }
    if v.tiling && !v.swipe {
        b.label(menu, "Swipe unavailable (event tap refused)");
    }

    // Looks.
    let overview = b.submenu(menu, &format!("Overview: {}", v.overview.label()));
    for m in OverviewMode::ALL {
        b.action(&overview, m.label(), v.overview == m, move || crate::with_desk(move |d| d.set_overview_mode(m)));
    }
    let (border, dim) = (v.border, v.dim);
    b.action(menu, "Focus Border", border, move || crate::with_desk(move |d| d.set_decor(!border, dim)));
    b.action(menu, "Dim Inactive Windows", dim, move || crate::with_desk(move |d| d.set_decor(border, !dim)));
    let gap = b.submenu(menu, &format!("Gap: {} pt", v.gap));
    for g in [0.0, 4.0, 8.0, 12.0, 16.0, 24.0, 32.0] {
        b.action(&gap, &format!("{g} pt"), (v.gap - g).abs() < 0.5, move || crate::with_desk(move |d| d.set_gap(g)));
    }
    sep(menu, mtm);

    b.action(menu, "Launch at Login", crate::login_item::is_enabled(), crate::login_item::toggle);
    if v.tiling {
        b.action(menu, "Re-scan Windows", false, || crate::with_desk(|d| d.rescan_all()));
    } else {
        b.action(menu, "Bring Back Off-screen Windows", false, || {
            crate::with_desk(|d| {
                d.rescue_offscreen_windows();
            })
        });
    }
    b.action(menu, "Open Log", false, crate::open_log);
    sep(menu, mtm);
    b.action(menu, "Quit Reel", false, crate::quit);

    MENU.with(|s| s.borrow_mut().as_mut().map(|st| st.targets = targets));
}
