# Continue — Reel (my-scroll-wm)
Handoff for a fresh session. Read this, then open only the files your task touches.
Reel = scrolling (niri-style) window manager for macOS, menu bar app, Rust + a SwiftUI panel.
Repo github.com/RasanKhabat/reel; Homebrew tap in `../homebrew-reel`. `../reel-rust` and `../reel-swift`
are old copies (not git) — ignore them.
Deep docs (only if needed): README.md (user-facing: shortcuts, install, dev), NOTES.md (design history,
animation experiments, bugs found; large — grep it, don't read whole).

## State (2026-11-11)
- Released v0.4.0 (Oct 25). After it, on main: Show desktop (⌃⌥D), flicker fix, left-edge resize fix,
  freeze fix for slow apps (Oct 29).
- 11-11 committed: back-off from unresponsive apps, resize-loop guard, lock/unlock/wake fixes, lost
  swipe-gesture recovery. Unit tests + clippy pass; the e2e harness was not run on it.

## Next
- Ask the user — no roadmap written. Likely: e2e run of the 11-11 fixes, then cut 0.4.1.

## How to work here
- Build: `./build.sh` → build/Reel.app (needs "Reel Local Signing" cert or macOS re-asks Accessibility).
- Check: `cargo test` and `cargo clippy` (must be clean). Release: push a `v*` tag (CI builds, publishes, updates tap).
- End-to-end: test mode `--test-pid <pid> --test-dir <dir>` drives Reel against dummy windows only.
  Never let a test move the user's real windows. Screen locked → AX sees no windows.
- Panel: `swift/Panel.swift` (display + actions only), Rust side `src/panel.rs`; `Reel --render-panel out.png`.
- Log: `~/Library/Logs/Reel/reel.log`. Commit when green; no Claude attribution. Keep reports short.

## Log (newest first, keep ≤8)
- 11-11 Committed the unresponsive-app back-off and lock/wake/gesture robustness fixes.
- 10-29 Show desktop, flicker/resize/freeze fixes.
- 10-25 v0.4.0: one ⇧-arrow keymap, ⅙…full width steps, animate-moves switch.
- 10-22 SwiftUI panel in the Rust app; demo-matched springs + animation experiments.
- 10-21 Rust rewrite (per-app mover threads), e2e harness 109/109.

<!-- Updating: when you finish, REWRITE State/Next (don't append), add one Log line, drop the oldest
past 8, keep the whole file under ~45 lines. Details belong in commits and NOTES.md, not here. -->
