//! Saving and restoring the arrangement. Window handles don't survive a restart, so
//! windows are re-identified heuristically: same app, then same process and/or title,
//! then similar size. The JSON format matches the Swift version's, so existing layouts
//! carry over.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WindowDescriptor {
    #[serde(rename = "bundleID")]
    pub bundle_id: Option<String>,
    pub pid: i32,
    pub title: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedColumn {
    pub windows: Vec<WindowDescriptor>,
    pub share: f64,
    pub tabbed: bool,
    pub row: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedStrip {
    pub columns: Vec<SavedColumn>,
    pub focus: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedLayout {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub tiling: bool,
    #[serde(default = "eight")]
    pub gap: f64,
    /// Keyed by display UUID (stable across reboots, unlike display numbers).
    #[serde(default)]
    pub strips: HashMap<String, SavedStrip>,
}

fn one() -> u32 {
    1
}
fn eight() -> f64 {
    8.0
}

impl Default for SavedLayout {
    fn default() -> Self {
        SavedLayout { version: 1, tiling: false, gap: 8.0, strips: HashMap::new() }
    }
}

pub fn file_path() -> PathBuf {
    if let Some(dir) = crate::test_mode::directory() {
        return dir.join("layout.json");
    }
    crate::support_dir().join("layout.json")
}

pub fn load() -> SavedLayout {
    std::fs::read(file_path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn save(layout: &SavedLayout) {
    let path = file_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(layout) {
        Ok(data) => {
            // Write-then-rename so a crash never leaves half a file.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
                crate::log::write("could not save layout");
            }
        }
        Err(e) => crate::log::write(&format!("could not encode layout: {e}")),
    }
}

/// How confidently `live` is the window `saved` described. 0 = not a match.
pub fn score(saved: &WindowDescriptor, live: &WindowDescriptor) -> u32 {
    if saved.bundle_id != live.bundle_id {
        return 0;
    }
    let same_pid = saved.pid == live.pid;
    let same_title = !saved.title.is_empty() && saved.title == live.title;
    if !same_pid && !same_title {
        return 0; // same app but nothing else agrees: too weak to trust
    }
    let mut s = 1;
    if same_pid {
        s += 4;
    }
    if same_title {
        s += 3;
    }
    if (saved.width - live.width).abs() < 4.0 && (saved.height - live.height).abs() < 4.0 {
        s += 1;
    }
    s
}

pub struct PlannedColumn<T> {
    pub items: Vec<T>,
    pub share: f64,
    pub tabbed: bool,
    pub row: usize,
}

pub struct RestorePlan<T> {
    pub columns: Vec<PlannedColumn<T>>,
    pub focus: usize,
    pub leftovers: Vec<T>,
}

/// Rebuilds the saved column order from live windows. Greedy best match per saved slot.
pub fn plan<T: Clone>(saved: &SavedStrip, live: &[(T, WindowDescriptor)]) -> RestorePlan<T> {
    let mut unused: Vec<usize> = (0..live.len()).collect();
    let mut columns = vec![];
    let mut index_for_saved: HashMap<usize, usize> = HashMap::new();
    for (si, col) in saved.columns.iter().enumerate() {
        let mut items = vec![];
        let mut row = 0;
        for (wi, desc) in col.windows.iter().enumerate() {
            let best = unused
                .iter()
                .map(|&i| (i, score(desc, &live[i].1)))
                .filter(|(_, s)| *s > 0)
                .max_by_key(|(_, s)| *s);
            let Some((li, _)) = best else { continue };
            unused.retain(|&i| i != li);
            if wi == col.row {
                row = items.len();
            }
            items.push(live[li].0.clone());
        }
        if items.is_empty() {
            continue;
        }
        index_for_saved.insert(si, columns.len());
        let tabbed = col.tabbed && items.len() > 1;
        columns.push(PlannedColumn { items, share: col.share, tabbed, row });
    }
    // Focus the saved focused column if it survived, else the nearest surviving one before it.
    let focus = (0..=saved.focus).rev().find_map(|i| index_for_saved.get(&i).copied()).unwrap_or(0);
    let leftovers = unused.iter().map(|&i| live[i].0.clone()).collect();
    RestorePlan { columns, focus, leftovers }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(bundle: &str, pid: i32, title: &str) -> WindowDescriptor {
        WindowDescriptor { bundle_id: Some(bundle.into()), pid, title: title.into(), width: 500.0, height: 800.0 }
    }

    #[test]
    fn score_needs_same_app_and_pid_or_title() {
        assert_eq!(score(&d("a", 1, "x"), &d("b", 1, "x")), 0);
        assert_eq!(score(&d("a", 1, "x"), &d("a", 2, "y")), 0);
        assert!(score(&d("a", 1, "x"), &d("a", 2, "x")) > 0);
        assert!(score(&d("a", 1, "x"), &d("a", 1, "y")) > 0);
    }

    #[test]
    fn plan_restores_order_stacks_and_focus() {
        let saved = SavedStrip {
            columns: vec![
                SavedColumn { windows: vec![d("term", 1, "shell")], share: 1.0 / 3.0, tabbed: false, row: 0 },
                SavedColumn { windows: vec![d("web", 1, "Docs"), d("web", 1, "Mail")], share: 2.0 / 3.0, tabbed: true, row: 1 },
                SavedColumn { windows: vec![d("gone", 1, "closed")], share: 0.5, tabbed: false, row: 0 },
            ],
            focus: 1,
        };
        let live = vec![
            ("mail", d("web", 9, "Mail")),
            ("new", d("notes", 7, "Untitled")),
            ("shell", d("term", 8, "shell")),
            ("docs", d("web", 9, "Docs")),
        ];
        let p = plan(&saved, &live);
        let cols: Vec<Vec<&str>> = p.columns.iter().map(|c| c.items.clone()).collect();
        assert_eq!(cols, vec![vec!["shell"], vec!["docs", "mail"]]);
        assert_eq!(p.columns[1].share, 2.0 / 3.0);
        assert!(p.columns[1].tabbed);
        assert_eq!(p.columns[1].row, 1);
        assert_eq!(p.focus, 1);
        assert_eq!(p.leftovers, vec!["new"]);
    }

    #[test]
    fn focus_falls_back_when_focused_column_is_gone() {
        let saved = SavedStrip {
            columns: vec![
                SavedColumn { windows: vec![d("a", 1, "1")], share: 0.5, tabbed: false, row: 0 },
                SavedColumn { windows: vec![d("b", 1, "2")], share: 0.5, tabbed: false, row: 0 },
            ],
            focus: 1,
        };
        assert_eq!(plan(&saved, &[("one", d("a", 1, "1"))]).focus, 0);
    }

    #[test]
    fn reads_swift_era_json() {
        let json = r#"{"gap":8,"strips":{"X":{"columns":[{"row":0,"share":0.5,"tabbed":false,
            "windows":[{"bundleID":"a","height":800,"pid":3,"title":"t","width":500}]}],"focus":0}},
            "tiling":true,"version":1}"#;
        let l: SavedLayout = serde_json::from_str(json).unwrap();
        assert!(l.tiling);
        assert_eq!(l.strips["X"].columns[0].windows[0].bundle_id.as_deref(), Some("a"));
        let back: SavedLayout = serde_json::from_slice(&serde_json::to_vec(&l).unwrap()).unwrap();
        assert_eq!(back, l);
    }
}
