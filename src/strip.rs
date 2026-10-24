//! Pure layout of one display's horizontally scrolling strip. No macOS calls here, so all
//! of it is unit-tested.
//!
//! Coordinates are "strip space": x = 0 is the strip's left edge, independent of scrolling;
//! y = 0 is the top of the usable area.

use std::collections::HashMap;

/// Opaque handle for a managed window (the controller maps it to the real window).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WinId(pub u64);

/// Opaque, stable column identity (for animation state that follows a column around).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColId(pub u64);

#[derive(Clone, Debug)]
pub struct Column {
    pub id: ColId,
    pub windows: Vec<WinId>,
    /// Width as a share of the viewport width.
    pub share: f64,
    /// Width the app actually accepted last time (apps may enforce a minimum or maximum).
    pub accepted_width: Option<f64>,
    /// Which window in the stack has focus.
    pub row: usize,
    /// Tabbed: every window takes the full column height, only the focused one is on top.
    pub tabbed: bool,
    /// Share to return to when leaving full width.
    pub share_before_full: Option<f64>,
}

impl Column {
    pub fn focused_window(&self) -> Option<WinId> {
        self.windows.get(self.row).copied()
    }
    pub fn shows_tabs(&self) -> bool {
        self.tabbed && self.windows.len() > 1
    }
}

pub const WIDTH_PRESETS: [f64; 4] = [1.0 / 3.0, 0.5, 2.0 / 3.0, 1.0];
/// The sizes ⌃⌥- / ⌃⌥= step through (share of the screen width).
pub const WIDTH_LADDER: [f64; 8] = [1.0 / 6.0, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75, 5.0 / 6.0, 1.0];

#[derive(Clone, Debug)]
pub struct Strip {
    pub columns: Vec<Column>,
    pub focus: usize,
    /// Strip-space x shown at the viewport's left edge.
    pub offset: f64,
    pub gap: f64,
    /// Height reserved above a tabbed column for its tab bar.
    pub tab_bar_height: f64,
    /// Viewport (usable screen area) size in points.
    pub viewport: (f64, f64),
    /// Each window's own width share from before it was stacked, so popping it out
    /// (or unstacking) gives it its old size back.
    pub share_before_stack: HashMap<WinId, f64>,
    next_col: u64,
}

impl Default for Strip {
    fn default() -> Self {
        Strip {
            columns: vec![],
            focus: 0,
            offset: 0.0,
            gap: 8.0,
            tab_bar_height: 26.0,
            viewport: (0.0, 0.0),
            share_before_stack: HashMap::new(),
            next_col: 1,
        }
    }
}

impl Strip {
    pub fn new_column(&mut self, windows: Vec<WinId>, share: f64) -> Column {
        let id = ColId(self.next_col);
        self.next_col += 1;
        Column { id, windows, share, accepted_width: None, row: 0, tabbed: false, share_before_full: None }
    }

    pub fn focused_column(&self) -> Option<&Column> {
        self.columns.get(self.focus)
    }

    pub fn focused_window(&self) -> Option<WinId> {
        self.focused_column().and_then(Column::focused_window)
    }

    // MARK: Geometry

    pub fn target_width(&self, share: f64) -> f64 {
        // Two half-width columns plus three gaps fill the viewport exactly.
        (share * (self.viewport.0 - self.gap) - self.gap).round()
    }

    pub fn width(&self, c: &Column) -> f64 {
        c.accepted_width.unwrap_or_else(|| self.target_width(c.share))
    }

    /// Strip-space left edge of each column, plus the strip's total width.
    pub fn column_edges(&self) -> (Vec<f64>, f64) {
        let mut x = self.gap;
        let mut xs = Vec::with_capacity(self.columns.len());
        for c in &self.columns {
            xs.push(x);
            x += self.width(c) + self.gap;
        }
        (xs, x)
    }

    pub fn max_offset(&self) -> f64 {
        (self.column_edges().1 - self.viewport.0).max(0.0)
    }

    /// Vertical placement (y, height) of each window in a column.
    pub fn rows(&self, c: &Column) -> Vec<(f64, f64)> {
        let top = self.gap;
        let available = self.viewport.1 - 2.0 * self.gap;
        if c.shows_tabs() {
            let y = top + self.tab_bar_height + 4.0;
            return c.windows.iter().map(|_| (y, available - self.tab_bar_height - 4.0)).collect();
        }
        let n = c.windows.len().max(1) as f64;
        let h = ((available - (n - 1.0) * self.gap) / n).floor();
        (0..c.windows.len()).map(|i| (top + i as f64 * (h + self.gap), h)).collect()
    }

    // MARK: Scrolling

    /// Smallest scroll change that puts the focused column (with its gaps) on screen.
    pub fn scroll_focus_into_view(&mut self) {
        let Some(col) = self.focused_column() else {
            self.offset = 0.0;
            return;
        };
        let x = self.column_edges().0[self.focus];
        let w = self.width(col);
        let mut o = self.offset;
        if x - self.gap < o {
            o = x - self.gap;
        } else if x + w + self.gap > o + self.viewport.0 {
            o = x + w + self.gap - self.viewport.0;
        }
        self.offset = o.clamp(0.0, self.max_offset());
    }

    /// Puts the focused column in the middle of the viewport.
    pub fn center_focused(&mut self) {
        let Some(col) = self.focused_column() else { return };
        let x = self.column_edges().0[self.focus];
        self.offset = (x + self.width(col) / 2.0 - self.viewport.0 / 2.0).round();
    }

    /// Where a flung strip should come to rest: the column start nearest to `projected`.
    pub fn resting_offset(&self, projected: f64) -> f64 {
        let max = self.max_offset();
        let mut stops: Vec<f64> = self.column_edges().0.iter().map(|x| (x - self.gap).clamp(0.0, max)).collect();
        stops.push(max);
        stops.into_iter().min_by(|a, b| (a - projected).abs().total_cmp(&(b - projected).abs())).unwrap_or(0.0)
    }

    /// Index of the first column fully inside the viewport at `offset`.
    pub fn first_fully_visible_column(&self, offset: f64) -> Option<usize> {
        let xs = self.column_edges().0;
        (0..self.columns.len()).find(|&i| xs[i] >= offset - 1.0 && xs[i] + self.width(&self.columns[i]) <= offset + self.viewport.0 + 1.0)
    }

    // MARK: Focus

    pub fn move_focus(&mut self, delta: isize) -> bool {
        let next = self.focus as isize + delta;
        if next < 0 || next as usize >= self.columns.len() {
            return false;
        }
        self.focus = next as usize;
        self.scroll_focus_into_view();
        true
    }

    pub fn focus_column_at(&mut self, index: usize) -> bool {
        if index >= self.columns.len() {
            return false;
        }
        self.focus = index;
        self.scroll_focus_into_view();
        true
    }

    pub fn move_focus_in_stack(&mut self, delta: isize) -> bool {
        let Some(c) = self.columns.get_mut(self.focus) else { return false };
        let next = c.row as isize + delta;
        if next < 0 || next as usize >= c.windows.len() {
            return false;
        }
        c.row = next as usize;
        true
    }

    /// Focuses a specific window. Returns false if it isn't in this strip.
    pub fn focus_on(&mut self, w: WinId) -> bool {
        let Some((c, r)) = self.locate(w) else { return false };
        self.focus = c;
        self.columns[c].row = r;
        self.scroll_focus_into_view();
        true
    }

    // MARK: Width

    /// Moves the focused column to the next (or previous) preset, wrapping around.
    pub fn cycle_focused_width(&mut self, forward: bool) -> Option<f64> {
        let current = self.columns.get(self.focus)?.share;
        let next = if forward {
            WIDTH_PRESETS.iter().copied().find(|p| *p > current + 0.01).unwrap_or(WIDTH_PRESETS[0])
        } else {
            WIDTH_PRESETS.iter().rev().copied().find(|p| *p < current - 0.01).unwrap_or(WIDTH_PRESETS[3])
        };
        self.set_share(next);
        Some(next)
    }

    /// Grows (positive) or shrinks (negative) the focused column to the next size on
    /// `WIDTH_LADDER`, `steps` sizes at a time. Off the ladder (e.g. after a manual resize)
    /// the first step goes to the nearest size in the direction of travel. Doesn't wrap;
    /// returns false when already at the smallest or largest size.
    pub fn adjust_focused_width(&mut self, steps: i32) -> bool {
        let Some(c) = self.columns.get(self.focus) else { return false };
        let current = c.share;
        let mut next = current;
        for _ in 0..steps.unsigned_abs() {
            let found = if steps > 0 {
                WIDTH_LADDER.iter().copied().find(|&s| s > next + 0.01)
            } else {
                WIDTH_LADDER.iter().rev().copied().find(|&s| s < next - 0.01)
            };
            match found {
                Some(s) => next = s,
                None => break,
            }
        }
        if (next - current).abs() <= 0.001 {
            return false;
        }
        self.set_share(next);
        true
    }

    /// Toggles the focused column between full width and its previous width.
    pub fn toggle_full_width(&mut self) {
        let Some(c) = self.columns.get_mut(self.focus) else { return };
        if c.share >= 0.99 {
            let back = c.share_before_full.unwrap_or(0.5);
            self.set_share(back);
        } else {
            c.share_before_full = Some(c.share);
            self.set_share(1.0);
        }
    }

    fn set_share(&mut self, share: f64) {
        let c = &mut self.columns[self.focus];
        c.share = share;
        c.accepted_width = None;
        self.scroll_focus_into_view();
    }

    // MARK: Editing

    /// Inserts a column directly right of the focused one and focuses it.
    pub fn insert_beside_focus(&mut self, windows: Vec<WinId>, share: f64) {
        let col = self.new_column(windows, share);
        let at = if self.columns.is_empty() { 0 } else { self.focus + 1 };
        self.columns.insert(at, col);
        self.focus = at;
        self.scroll_focus_into_view();
    }

    /// Removes a window; drops its column if it empties. When the focused column
    /// disappears, focus goes to the column that slides into its place (the right
    /// neighbour), or the left one at the end of the strip. Returns the column index.
    pub fn remove_window(&mut self, w: WinId) -> Option<usize> {
        let (ci, ri) = self.locate(w)?;
        self.share_before_stack.remove(&w);
        let col = &mut self.columns[ci];
        col.windows.remove(ri);
        if col.windows.is_empty() {
            self.columns.remove(ci);
            if self.focus > ci {
                self.focus -= 1;
            }
            self.focus = self.focus.min(self.columns.len().saturating_sub(1));
        } else {
            if ri < col.row {
                col.row -= 1;
            }
            col.row = col.row.min(col.windows.len() - 1);
            if col.windows.len() < 2 {
                col.tabbed = false;
            }
        }
        self.scroll_focus_into_view();
        Some(ci)
    }

    /// Swaps the focused column with its neighbour; focus travels with it.
    pub fn move_focused_column(&mut self, delta: isize) -> bool {
        let to = self.focus as isize + delta;
        if self.focus >= self.columns.len() || to < 0 || to as usize >= self.columns.len() {
            return false;
        }
        self.columns.swap(self.focus, to as usize);
        self.focus = to as usize;
        self.scroll_focus_into_view();
        true
    }

    /// Moves the focused window up or down within its stack.
    pub fn move_focused_window_in_stack(&mut self, delta: isize) -> bool {
        let Some(c) = self.columns.get_mut(self.focus) else { return false };
        let to = c.row as isize + delta;
        if to < 0 || to as usize >= c.windows.len() {
            return false;
        }
        c.windows.swap(c.row, to as usize);
        c.row = to as usize;
        true
    }

    /// Moves the focused window into the neighbouring column (bottom of its stack).
    pub fn stack_focused_window(&mut self, delta: isize) -> bool {
        let target = self.focus as isize + delta;
        if self.focus >= self.columns.len() || target < 0 || target as usize >= self.columns.len() {
            return false;
        }
        let Some(win) = self.columns[self.focus].focused_window() else { return false };
        let mut t = target as usize;
        // Remember its own width (if it had a column to itself) for when it leaves the stack.
        if self.columns[self.focus].windows.len() == 1 {
            self.share_before_stack.insert(win, self.columns[self.focus].share);
        }
        let src = &mut self.columns[self.focus];
        let row = src.row;
        src.windows.remove(row);
        if src.windows.is_empty() {
            self.columns.remove(self.focus);
            if t > self.focus {
                t -= 1;
            }
        } else {
            src.row = src.row.min(src.windows.len() - 1);
            if src.windows.len() < 2 {
                src.tabbed = false;
            }
        }
        let dst = &mut self.columns[t];
        dst.windows.push(win);
        dst.row = dst.windows.len() - 1;
        self.focus = t;
        self.scroll_focus_into_view();
        true
    }

    /// Takes the focused window out of its stack into a new column on the given side.
    pub fn expel_focused_window(&mut self, delta: isize) -> bool {
        let Some(c) = self.columns.get(self.focus) else { return false };
        if c.windows.len() < 2 {
            return false;
        }
        let win = c.windows[c.row];
        let share = self.share_before_stack.remove(&win).unwrap_or(c.share);
        let src = &mut self.columns[self.focus];
        let row = src.row;
        src.windows.remove(row);
        src.row = src.row.min(src.windows.len() - 1);
        if src.windows.len() < 2 {
            src.tabbed = false;
        }
        let at = if delta < 0 { self.focus } else { self.focus + 1 };
        let col = self.new_column(vec![win], share);
        self.columns.insert(at, col);
        self.focus = at;
        self.scroll_focus_into_view();
        true
    }

    /// Splits every stack so each window gets its own column, keeping left-to-right order
    /// (a stack's windows become consecutive columns, top first). Returns false if there
    /// was nothing to split.
    pub fn unstack_all(&mut self) -> bool {
        if !self.columns.iter().any(|c| c.windows.len() > 1) {
            return false;
        }
        let focused = self.focused_window();
        let old = std::mem::take(&mut self.columns);
        for c in old {
            if c.windows.len() <= 1 {
                self.columns.push(c);
                continue;
            }
            for w in c.windows {
                let share = self.share_before_stack.remove(&w).unwrap_or(c.share);
                let col = self.new_column(vec![w], share);
                self.columns.push(col);
            }
        }
        if let Some(fw) = focused {
            if let Some(i) = self.columns.iter().position(|c| c.windows[0] == fw) {
                self.focus = i;
            }
        }
        self.scroll_focus_into_view();
        true
    }

    pub fn toggle_tabbed(&mut self) -> bool {
        let Some(c) = self.columns.get_mut(self.focus) else { return false };
        if c.windows.len() < 2 {
            return false;
        }
        c.tabbed = !c.tabbed;
        true
    }

    // MARK: Lookup

    pub fn locate(&self, w: WinId) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(c, col)| col.windows.iter().position(|x| *x == w).map(|r| (c, r)))
    }

    pub fn all_windows(&self) -> impl Iterator<Item = WinId> + '_ {
        self.columns.iter().flat_map(|c| c.windows.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(layout: &[&[u64]], focus: usize) -> Strip {
        let mut s = Strip { viewport: (1000.0, 808.0), ..Default::default() };
        for ws in layout {
            let c = s.new_column(ws.iter().map(|&n| WinId(n)).collect(), 0.5);
            s.columns.push(c);
        }
        s.focus = focus;
        s
    }
    fn plain(n: u64) -> Strip {
        let ids: Vec<Vec<u64>> = (1..=n).map(|i| vec![i]).collect();
        let refs: Vec<&[u64]> = ids.iter().map(|v| v.as_slice()).collect();
        let mut s = strip(&refs, 0);
        s.viewport = (1000.0, 800.0);
        s
    }
    fn names(s: &Strip) -> Vec<Vec<u64>> {
        s.columns.iter().map(|c| c.windows.iter().map(|w| w.0).collect()).collect()
    }
    fn focused(s: &Strip) -> Option<u64> {
        s.focused_window().map(|w| w.0)
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn two_halves_fill_viewport() {
        let s = plain(2);
        assert_eq!(s.column_edges(), (vec![8.0, 504.0], 1000.0));
    }

    #[test]
    fn focus_right_scrolls_minimally() {
        let mut s = plain(3);
        assert!(s.move_focus(1));
        assert_eq!(s.offset, 0.0);
        assert!(s.move_focus(1));
        assert_eq!(s.offset, 496.0);
        assert!(!s.move_focus(1));
        assert!(s.move_focus(-2));
        assert_eq!(s.offset, 0.0);
    }

    #[test]
    fn cycle_width_wraps_both_ways() {
        let mut s = plain(1);
        assert_eq!(s.cycle_focused_width(true), Some(2.0 / 3.0));
        assert_eq!(s.cycle_focused_width(true), Some(1.0));
        assert_eq!(s.cycle_focused_width(true), Some(1.0 / 3.0));
        assert_eq!(s.cycle_focused_width(false), Some(1.0));
    }

    #[test]
    fn accepted_width_overrides_target() {
        let mut s = plain(2);
        s.columns[0].accepted_width = Some(600.0);
        assert_eq!(s.column_edges().0[1], 616.0);
        s.columns[0].accepted_width = Some(300.0);
        assert_eq!(s.column_edges().0[1], 316.0);
        s.cycle_focused_width(true);
        assert_eq!(s.columns[0].accepted_width, None);
    }

    #[test]
    fn width_steps_and_limits() {
        let mut s = plain(2);
        s.viewport = (1200.0, 800.0);
        assert!(s.adjust_focused_width(1)); // ½ → ⅔
        assert!(close(s.columns[0].share, 2.0 / 3.0));
        assert!(s.adjust_focused_width(-2)); // ⅔ → ½ → ⅓
        assert!(close(s.columns[0].share, 1.0 / 3.0));
        s.columns[0].share = 1.0;
        assert!(!s.adjust_focused_width(1)); // no wrapping
        s.columns[0].share = 1.0 / 6.0;
        assert!(!s.adjust_focused_width(-1));
        s.columns[0].share = 0.55; // off the ladder: nearest size in that direction
        assert!(s.adjust_focused_width(1));
        assert!(close(s.columns[0].share, 2.0 / 3.0));
        s.columns[0].share = 0.55;
        assert!(s.adjust_focused_width(-1));
        assert!(close(s.columns[0].share, 0.5));
        s.columns[0].share = 0.75;
        assert!(s.adjust_focused_width(3)); // ¾ → ⅚ → full, then stops
        assert!(close(s.columns[0].share, 1.0));
    }

    #[test]
    fn insert_beside_focus() {
        let mut s = strip(&[&[1], &[2], &[3]], 0);
        s.insert_beside_focus(vec![WinId(9)], 0.5);
        assert_eq!(names(&s), vec![vec![1], vec![9], vec![2], vec![3]]);
        assert_eq!(s.focus, 1);
        let mut e = strip(&[], 0);
        e.insert_beside_focus(vec![WinId(9)], 0.5);
        assert_eq!(e.focus, 0);
    }

    #[test]
    fn close_focus_rules() {
        let mut s = strip(&[&[1], &[2], &[3]], 1);
        s.remove_window(WinId(2));
        assert_eq!(focused(&s), Some(3));
        let mut s = strip(&[&[1], &[2], &[3]], 2);
        s.remove_window(WinId(3));
        assert_eq!(focused(&s), Some(2));
        let mut s = strip(&[&[1], &[2], &[3]], 2);
        s.remove_window(WinId(1));
        assert_eq!(focused(&s), Some(3));
        assert_eq!(s.remove_window(WinId(42)), None);
    }

    #[test]
    fn move_column() {
        let mut s = strip(&[&[1], &[2], &[3]], 0);
        assert!(s.move_focused_column(1));
        assert_eq!(names(&s), vec![vec![2], vec![1], vec![3]]);
        assert_eq!(s.focus, 1);
        assert!(!s.move_focused_column(-2));
    }

    #[test]
    fn stack_rows_and_tabs() {
        let s = strip(&[&[1, 2]], 0);
        assert_eq!(s.rows(&s.columns[0]), vec![(8.0, 392.0), (408.0, 392.0)]);
        let mut t = strip(&[&[1, 2, 3]], 0);
        assert!(t.toggle_tabbed());
        let rows = t.rows(&t.columns[0]);
        assert!(rows.iter().all(|r| r.0 == 38.0 && r.1 == 808.0 - 16.0 - 30.0));
        assert!(!strip(&[&[1]], 0).toggle_tabbed());
    }

    #[test]
    fn stack_into_neighbours() {
        let mut s = strip(&[&[1], &[2], &[3]], 1);
        assert!(s.stack_focused_window(-1));
        assert_eq!(names(&s), vec![vec![1, 2], vec![3]]);
        assert_eq!(focused(&s), Some(2));
        let mut s = strip(&[&[1], &[2], &[3]], 0);
        assert!(s.stack_focused_window(1));
        assert_eq!(names(&s), vec![vec![2, 1], vec![3]]);
        assert!(!strip(&[&[1], &[2]], 0).stack_focused_window(-1));
    }

    #[test]
    fn expel_and_unstack() {
        let mut s = strip(&[&[1, 2, 3]], 0);
        s.columns[0].row = 1;
        assert!(s.expel_focused_window(1));
        assert_eq!(names(&s), vec![vec![1, 3], vec![2]]);
        s.focus = 0;
        s.columns[0].row = 0;
        assert!(s.expel_focused_window(-1));
        assert_eq!(names(&s), vec![vec![1], vec![3], vec![2]]);
        assert!(!strip(&[&[1]], 0).expel_focused_window(1));

        let mut u = strip(&[&[1, 2], &[3, 4], &[5]], 1);
        u.columns[0].share = 1.0 / 3.0;
        u.columns[1].row = 1;
        u.columns[1].tabbed = true;
        assert!(u.unstack_all());
        assert_eq!(names(&u), vec![vec![1], vec![2], vec![3], vec![4], vec![5]]);
        assert!(!u.columns.iter().any(|c| c.tabbed));
        assert_eq!(focused(&u), Some(4));
        assert!(!u.unstack_all());
    }

    #[test]
    fn stack_then_pop_restores_width_and_height() {
        let mut s = strip(&[&[1], &[2]], 1);
        s.viewport = (1000.0, 800.0);
        s.columns[1].share = 2.0 / 3.0;
        assert!(s.stack_focused_window(-1));
        assert!(s.expel_focused_window(1));
        assert_eq!(s.columns.iter().map(|c| c.share).collect::<Vec<_>>(), vec![0.5, 2.0 / 3.0]);
        assert_eq!(s.rows(&s.columns[0]), vec![(8.0, 784.0)]);
        let mut u = strip(&[&[1], &[2]], 1);
        u.columns[1].share = 1.0 / 3.0;
        u.stack_focused_window(-1);
        u.unstack_all();
        assert_eq!(u.columns.iter().map(|c| c.share).collect::<Vec<_>>(), vec![0.5, 1.0 / 3.0]);
    }

    #[test]
    fn focus_and_move_within_stack() {
        let mut s = strip(&[&[1, 2, 3]], 0);
        assert!(s.move_focus_in_stack(1));
        assert_eq!(focused(&s), Some(2));
        assert!(s.move_focused_window_in_stack(1));
        assert_eq!(names(&s), vec![vec![1, 3, 2]]);
        assert!(!s.move_focus_in_stack(1));
        let mut r = strip(&[&[1, 2, 3]], 0);
        r.columns[0].row = 2;
        r.remove_window(WinId(1));
        assert_eq!(focused(&r), Some(3));
        let mut t = strip(&[&[1, 2]], 0);
        t.toggle_tabbed();
        t.remove_window(WinId(2));
        assert!(!t.columns[0].tabbed);
    }

    #[test]
    fn full_width_center_resting() {
        let mut s = strip(&[&[1]], 0);
        s.columns[0].share = 1.0 / 3.0;
        s.toggle_full_width();
        assert_eq!(s.columns[0].share, 1.0);
        s.toggle_full_width();
        assert_eq!(s.columns[0].share, 1.0 / 3.0);

        let mut c = strip(&[&[1], &[2], &[3]], 1);
        c.columns[1].share = 1.0 / 3.0;
        c.center_focused();
        let x = c.column_edges().0[1];
        assert_eq!(c.offset, (x + c.width(&c.columns[1]) / 2.0 - 500.0).round());

        let r = plain(4);
        assert_eq!(r.resting_offset(300.0), 496.0);
        assert_eq!(r.resting_offset(200.0), 0.0);
        assert_eq!(r.resting_offset(5000.0), 992.0);
        assert_eq!(r.resting_offset(-400.0), 0.0);
        let v = plain(3);
        assert_eq!(v.first_fully_visible_column(496.0), Some(1));
        assert_eq!(v.first_fully_visible_column(0.0), Some(0));
        let mut f = plain(2);
        assert!(f.focus_column_at(1));
        assert!(!f.focus_column_at(5));
    }
}
