//! The menu bar panel. Its UI is SwiftUI (`swift/Panel.swift`, compiled by build.rs into a
//! static library); this side gives it the current state as JSON and carries out the
//! actions it sends back. Everything runs on the main thread.

use crate::prefs::{OverviewMode, ScrollModifier};
use crate::shortcuts::{Command, Preset, MODIFIER_LEGEND};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

extern "C" {
    fn reel_panel_install(
        state: extern "C" fn() -> *mut c_char,
        free: extern "C" fn(*mut c_char),
        action: extern "C" fn(*const c_char, *const c_char),
    );
    fn reel_panel_close();
    fn reel_panel_render(json: *const c_char, path: *const c_char) -> bool;
}

thread_local! {
    /// Windows listed while tiling is off; refreshed when the panel opens.
    static WINDOWS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    /// The last state handed out, reused if the desk is busy at that moment.
    static LAST: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn install() {
    unsafe { reel_panel_install(state, free, action) };
}

/// Closes the panel if it's open (a shortcut fired while it was showing).
pub fn close() {
    unsafe { reel_panel_close() };
}

/// Development aid: draws the panel with the current settings into a PNG. With
/// `tiling`, shows the tiling view with a few sample columns.
pub fn render_preview(path: &str, tiling: bool) -> bool {
    refresh_windows();
    let Some(mut v) = read() else { return false };
    if tiling {
        v["tiling"] = json!(true);
        v["hasStacks"] = json!(true);
        v["strips"] = json!([{ "name": "Columns", "rows": [
            { "label": "Safari — Apple", "detail": "50%", "focused": false },
            { "label": "Terminal — zsh", "detail": "50% · stack of 2", "focused": true },
            { "label": "Notes", "detail": "33%", "focused": false },
        ]}]);
    }
    let (Ok(json), Ok(path)) = (CString::new(v.to_string()), CString::new(path)) else { return false };
    unsafe { reel_panel_render(json.as_ptr(), path.as_ptr()) }
}

extern "C" fn state() -> *mut c_char {
    let text = match read() {
        Some(v) => {
            let t = v.to_string();
            LAST.with(|l| *l.borrow_mut() = t.clone());
            t
        }
        None => LAST.with(|l| l.borrow().clone()),
    };
    if text.is_empty() {
        return std::ptr::null_mut();
    }
    CString::new(text).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

extern "C" fn free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

extern "C" fn action(name: *const c_char, value: *const c_char) {
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
    let value = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    perform(&name, &value);
}

fn read() -> Option<Value> {
    crate::with_desk_sync(|d| {
        let multi = d.strips.len() > 1;
        let strips: Vec<Value> = d
            .strips
            .iter()
            .map(|s| {
                let rows: Vec<Value> = s
                    .strip
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let w = c.focused_window().and_then(|id| s.wins.get(&id));
                        let label = w
                            .map(|w| {
                                let t = w.title();
                                if t.is_empty() { w.app.clone() } else { format!("{} — {}", w.app, t) }
                            })
                            .unwrap_or_else(|| "?".into());
                        let mut detail = format!("{}%", (c.share * 100.0).round());
                        if c.windows.len() > 1 {
                            detail += &if c.tabbed { format!(" · {} tabs", c.windows.len()) } else { format!(" · stack of {}", c.windows.len()) };
                        }
                        json!({ "label": label, "detail": detail, "focused": i == s.strip.focus })
                    })
                    .collect();
                json!({ "name": if multi { s.display.name.clone() } else { "Columns".into() }, "rows": rows })
            })
            .collect();
        let bindings = d.bindings();
        let groups: Vec<Value> = Command::groups()
            .into_iter()
            .map(|(name, commands)| {
                let items: Vec<Value> = commands
                    .into_iter()
                    .map(|c| json!({ "title": c.title(), "keys": bindings.get(&c).map(|k| k.spelled()).unwrap_or_else(|| "— none —".into()) }))
                    .collect();
                json!({ "name": name, "items": items })
            })
            .collect();
        json!({
            "trusted": crate::ax::is_trusted(),
            "tiling": d.is_tiling,
            "launchAtLogin": crate::login_item::is_enabled(),
            "swipeAvailable": !d.is_tiling || d.swipe_available,
            "hasStacks": d.strips.iter().any(|s| s.strip.columns.iter().any(|c| c.windows.len() > 1)),
            "unstackKeys": bindings.get(&Command::UnstackAll).map(|k| k.glyphs()),
            "preset": d.preset.raw(),
            "scrollModifier": d.scroll_modifier.raw(),
            "scrollSpelled": d.scroll_modifier.spelled(),
            "scrollCaveat": d.scroll_modifier.caveat(),
            "scrollSpeed": d.scroll_speed,
            "overview": d.overview_mode.raw(),
            "border": d.show_border,
            "dim": d.dim_inactive,
            "gap": d.gap(),
            "failed": d.failed_shortcuts.iter().map(|c| c.glyphs()).collect::<Vec<_>>(),
            "strips": strips,
            "legend": MODIFIER_LEGEND,
            "groups": groups,
            "windows": if d.is_tiling { vec![] } else { WINDOWS.with(|w| w.borrow().clone()) },
        })
    })
}

fn perform(name: &str, value: &str) {
    let on = value == "1";
    match name {
        "tiling" => crate::set_tiling(on),
        "launchAtLogin" => {
            if on != crate::login_item::is_enabled() {
                crate::login_item::toggle();
            }
        }
        "preset" => {
            if let Some(p) = Preset::ALL.into_iter().find(|p| p.raw() == value) {
                crate::with_desk(move |d| d.set_preset(p));
                if p == Preset::Custom {
                    close();
                    crate::editor::show();
                }
            }
        }
        "editShortcuts" => {
            close();
            crate::editor::show();
        }
        "scrollModifier" => {
            if let Some(m) = ScrollModifier::ALL.into_iter().find(|m| m.raw() == value) {
                crate::with_desk(move |d| d.set_scroll_modifier(m));
            }
        }
        "scrollSpeed" => {
            if let Ok(v) = value.parse::<f64>() {
                crate::with_desk(move |d| d.set_scroll_speed(v.clamp(1.0, 4.0)));
            }
        }
        "overview" => {
            if let Some(m) = OverviewMode::ALL.into_iter().find(|m| m.raw() == value) {
                crate::with_desk(move |d| d.set_overview_mode(m));
            }
        }
        "border" => crate::with_desk(move |d| {
            let dim = d.dim_inactive;
            d.set_decor(on, dim)
        }),
        "dim" => crate::with_desk(move |d| {
            let border = d.show_border;
            d.set_decor(border, on)
        }),
        "gap" => {
            if let Ok(v) = value.parse::<f64>() {
                crate::with_desk(move |d| d.set_gap(v.clamp(0.0, 32.0)));
            }
        }
        "select" => {
            let mut it = value.split(',').filter_map(|x| x.parse::<usize>().ok());
            if let (Some(si), Some(ci)) = (it.next(), it.next()) {
                crate::with_desk(move |d| {
                    if let Some(s) = d.strips.get_mut(si) {
                        let row = s.strip.columns.get(ci).map(|c| c.row).unwrap_or(0);
                        s.select(ci, row);
                    }
                    crate::ensure_clock();
                });
            }
        }
        "unstackAll" => crate::with_desk(|d| d.unstack_all()),
        "rescue" => {
            crate::with_desk(|d| {
                d.rescue_offscreen_windows();
            });
            refresh_windows();
        }
        "refresh" => {
            refresh_windows();
            crate::with_desk(|d| d.rescan_all());
        }
        "refreshWindows" => refresh_windows(),
        "openLog" => crate::open_log(),
        "openAccessibility" => crate::open_accessibility_settings(),
        "quit" => crate::quit(),
        _ => crate::log::write(&format!("panel: unknown action {name}")),
    }
}

/// Lists every window of the apps Reel manages (shown while tiling is off).
fn refresh_windows() {
    let mut list: Vec<(String, String, Value)> = vec![];
    for app in crate::apps::managed() {
        let pid = app.processIdentifier();
        let name = app.localizedName().map(|s| s.to_string()).unwrap_or_else(|| format!("pid {pid}"));
        let el = crate::ax::Element::application(pid);
        el.set_timeout(0.5);
        for w in el.windows() {
            if w.role().as_deref() != Some(crate::ax::WINDOW_ROLE) {
                continue;
            }
            let Some(f) = w.frame() else { continue };
            let title = w.title();
            let v = json!({
                "title": title, "app": name, "x": f.x, "y": f.y, "w": f.w, "h": f.h,
                "standard": w.subrole().as_deref() == Some(crate::ax::STANDARD_WINDOW),
                "minimized": w.is_minimized(),
            });
            list.push((name.clone(), title, v));
        }
    }
    list.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    WINDOWS.with(|w| *w.borrow_mut() = list.into_iter().map(|(_, _, v)| v).collect());
}
