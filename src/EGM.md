# Reel core (Rust)

## Overview

The Rust crate is the whole window manager. `main.rs` sets Reel up as a menu bar (Accessory) app and
keeps the main-thread state. `Desk` (`desk.rs`) coordinates everything and owns one
`StripController` (`controller.rs`) per display. Each controller holds a pure layout model, `Strip`
(`strip.rs`), and pushes it onto real windows through Accessibility (`ax.rs`). Sizes are set right
away; positions glide on springs (`spring.rs`) driven by a frame clock (`clock.rs`) and posted to
per-app mover threads (`mover.rs`). Input arrives from global hotkeys and a scroll-gesture event tap
(`input.rs`), window changes from AX observers (`events.rs`). The menu bar panel is Swift
([`../swift/EGM.md`](../swift/EGM.md)) bridged by `panel.rs`.

| File | Role |
| --- | --- |
| `main.rs` | Entry point; thread-local `Desk`/`Mover`; `with_desk`, `later`; signals, screen and unlock notifications; trust wait; `--render-panel`. |
| `desk.rs` | `Desk`: tiling on/off, one strip per display, routes events, input, commands and settings; saves/restores layout. |
| `controller.rs` | `StripController`: applies a strip to real windows (AX sizes, spring positions, parking, tab bars, border/veils, overview, size verification). |
| `strip.rs` | `Strip`: macOS-free model of one display's scrolling strip (columns, stacks, tabs, widths, focus, offset). |
| `mover.rs` | `Mover`: one `reel-mover-<pid>` thread per app with a latest-value mailbox for AX moves. |
| `ax.rs` | `Element`: safe `AXUIElement` wrapper; per-pid back-off for apps that stop answering. |
| `events.rs` | AX observers and NSWorkspace observers → `WindowEvent` on the main thread. |
| `input.rs` | Carbon hotkeys, keyboard-tap fallback, scroll-gesture tap → `Input` on the main thread. |
| `shortcuts.rs` | `Command` (bindable actions), `Combo`, `Preset` keymaps, key/modifier constants. |
| `overlays.rs` | AppKit overlays: `Overview` minimap pill, `Ring` border/veil, `TabBar`. |
| `spring.rs` | Critically damped `Spring`, global animation speed and on/off flags. |
| `clock.rs` | Frame driver (CADisplayLink or 120 Hz timer) calling `Desk::tick`. |
| `layout_store.rs` | `layout.json` save/load and score-based rematching of saved windows to live ones. |
| `prefs.rs` | NSUserDefaults: keymap preset, custom bindings, generic getters/setters. |
| `panel.rs` | Rust side of the Swift panel: state JSON out, actions in. |
| `editor.rs` | "Reel Shortcuts" window for recording custom shortcuts. |
| `geometry.rs` | `Rect`, AX↔AppKit coordinate conversion, `Display` list keyed by display UUID. |
| `apps.rs` | Which apps Reel manages; app info; minimize a front window. |
| `experiments.rs` | `Exp` animation switches (off by default), stored as `exp.<key>` prefs. |
| `watchdog.rs` | Detached shell loop that relaunches Reel with `--recover` after an unclean exit. |
| `test_mode.rs`, `testctl.rs` | Test mode (`--test-pid` + `--test-dir`) and its notification command channel. |
| `login_item.rs` | Launch at Login via `SMAppService`. |
| `log.rs` | Timestamped `reel.log`. |
| `ffi.rs`, `objc_classes.rs` | Raw CF/AX/Quartz/Carbon/GCD bindings; ObjC classes that forward to Rust closures. |

## Interface

- `main.rs`: `with_desk(f)` (defers via `ffi::main_async` if the Desk is borrowed),
  `with_desk_sync(f) -> Option` (never blocks), `with_mover`, `later(secs, f)`, `ensure_clock`,
  `set_tiling`, `quit`, `screen_locked`, `support_dir`, `mouse_location_ax`.
- `desk.rs:Desk`: `start_tiling` / `stop_tiling(remember)`, `handle(WindowEvent)`, `input(Input)`,
  `perform(Command)`, `rescan_all`, `screens_changed`, `tick`, `toggle_desktop`, `unstack_all`,
  `rescue_offscreen_windows`, `schedule_save` / `save_now`, settings setters (`set_preset`,
  `set_custom_binding`, `set_scroll_*`, `set_animation*`, `set_overview_mode`, `set_decor`, `set_gap`).
- `controller.rs:StripController`: `rebuild`, `stop`, `commit`, `tick`, `verify_sizes`, `snapshot`,
  strip commands (`focus_column`, `move_column`, `stack`, `expel`, `toggle_tabbed`, `cycle_width`,
  `adjust_width`, `toggle_full_width`, `center`, …), event handlers (`remove`, `remove_app`,
  `follow_system_focus`, `window_resized`), trackpad drag (`begin_drag`/`drag`/`end_drag`),
  `set_desktop`. Also `Win`, `arm_focus_guard`/`focus_guard_armed`, `pending_checks`.
- `strip.rs:Strip`, `Column`, `WinId`, `ColId`, `WIDTH_PRESETS` (⅓, ½, ⅔, 1), `WIDTH_LADDER` (⅙…1).
- `mover.rs:Mover`: `post`, `post_frame`, `forget`, `forget_all`, `flush(timeout)`, `is_idle`.
- `ax.rs:Element` (attribute get/set, `frame`, `is_tileable`, `is_gone`, `system_focused_window`),
  `is_trusted`, `request_trust`, `app_unresponsive(pid)`.
- `events.rs`: `WindowEvent`, `start(sink)`, `stop`, `watch(window, pid)`.
- `input.rs`: `Input`, `set_sink`, `install`/`uninstall` (returns refused combos), `install_fixed`,
  `set_scroll_enabled`, `set_scroll_flags`, `start_scroll_tap`.
- `shortcuts.rs`: `Command` (35 actions), `Combo` (`problem()` explains invalid combos), `Preset`.
- `layout_store.rs`: `SavedLayout`/`SavedStrip`/`SavedColumn`/`WindowDescriptor`, `load`, `save`,
  `score`, `plan`.
- CLI: `--render-panel <file.png> [--tiling]` (`main.rs`), `--test-pid <pid> --test-dir <dir>`
  (`test_mode.rs`), `--recover` (`watchdog.rs`).

## Invariants

- **Main thread owns all state.** `Desk`, `Mover`, controller globals, events, hotkeys and the editor
  are `thread_local`; `mtm()` panics off the main thread. Event-tap and mover threads never touch
  them: taps only decide swallow/pass and forward via `main_async_send`.
- **Re-entrancy:** `with_desk` uses `try_borrow_mut` and re-queues instead of panicking; delayed
  callbacks look the strip up again by display key and do nothing if it is gone.
- **A hung app must never freeze Reel.** AX timeouts: 0.5 s system-wide, 0.3 s per managed window,
  0.1 s for enhanced-UI toggles. A pid whose call fails with `kAXErrorCannotComplete` after ≥90 ms
  is backed off for 3 s; on the main thread its calls fail fast. Unresponsive apps' windows are
  not treated as closed; `Mover::flush` does not wait for them.
- **Moves are latest-value:** each mover mailbox keeps only the newest frame per window, so slow apps
  skip intermediate frames. Moves are applied once, with no retry.
- **Tiling gate:** `handle`, `rescan_all`, `screens_changed` and `save_now` are no-ops while tiling is
  off; `stop_tiling` saves before clearing strips and brings parked windows back on screen.
- **Lock/wake:** a locked screen hides all windows from AX, so resizes are ignored while locked and
  display changes wait for unlock; unchanged displays only reapply (no re-sort).
- **Display-change retries:** a real display change records the windows held (`pending_expected`);
  while a rescan finds fewer, `display_retry` scans again (1, 2, 4, 8 s, waiting while locked) and
  rebuilds from `retry_reference` only on a bigger scan that includes every held window.
  `save_now` doesn't shrink a saved strip while a retry is pending; `display_generation` cancels
  stale retries. While locked the cycle waits without using attempts (and keeps the fuller saved
  layout until unlock).
- **Focus guard:** for 400 ms after Reel moves focus, system focus notifications are ignored.
- **Resize loops:** after 8 self-resizes within 3 s Reel accepts the app's size; sizes the app
  insists on are recorded as `settled` and not re-corrected.
- **Parking:** off-screen columns keep a 2 pt sliver at the side edge, or drop below the bottom when
  another display touches that edge.
- **Removing a window** must clear every per-window map in `StripController::forget`.
- **Animation:** every animation starts via `start_animating` so the clock runs; `tick` caps `dt`
  at 1/30 s; a strip is settled only when all springs and width animations are settled and no drag
  is active. Reduce Motion or animations off → snap.
- **Persistence:** `layout.json` keeps the Swift version's format, is written via `.tmp` + rename,
  and keeps layouts of disconnected displays. Saves are debounced to 1 s.
- **Test isolation:** in test mode Reel manages only the test pid, writes logs/state/layout into the
  test dir, installs no hotkeys or scroll tap, and never moves the user's real windows.
- `strip.rs` is pure: no OS calls; offset is clamped to `[0, max_offset]`.

## Side Effects

- AX reads and writes on other apps' windows (position, size, focus, raise, minimize, the
  undocumented `AXEnhancedUserInterface`), from the main thread and from `reel-mover-<pid>` threads.
- Global input: Carbon hotkeys (`REEL`, `REXT`), session `CGEventTap`s on threads `reel-keys` and
  `reel-scroll` that swallow matching events.
- AppKit: status item/popover (Swift), overlay panels, shortcuts window, `NSBeep`.
- Files: `~/Library/Application Support/Reel/layout.json`, `clean-exit`, `last-recovery`;
  `~/Library/Logs/Reel/reel.log` (truncated past 512,000 bytes); test-dir equivalents in test mode.
- NSUserDefaults: `keymapPreset`, `customBindings`, `customBindingsKnown`, `scrollModifier`,
  `scrollSpeed`, `animationsOff`, `animationSpeed`, `overviewMode`, `showFocusBorder`,
  `dimInactive`, `exp.<key>`.
- Processes: a detached `/bin/sh` watchdog that may run `open … --args --recover`; Launch at Login via
  `SMAppService`.
- Signals: SIGTERM/SIGINT/SIGHUP set a flag polled every 0.25 s, then `quit()` restores windows.

## Verification

- Run: `cargo test` and `cargo clippy` (must be clean).
- End-to-end: test mode `--test-pid <pid> --test-dir <dir>` against dummy windows only.
