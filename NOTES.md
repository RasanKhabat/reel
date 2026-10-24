# NOTES

## Scope
Only the public web demo at https://chainyourmac.com/#demo was studied, via REA over
Chrome CDP (127.0.0.1:9222). The ChainYourMac macOS app and installer were **not** touched.
Their demo describes *a browser simulation*; it is not evidence of how the real app moves windows.

## Evidence index
| ID | What |
|---|---|
| `ev_1ca18a5b…6adc6693` | Target list: page `904A36FB…` = chainyourmac.com |
| `ev_684a82f4…bf6d2196` | Bundle analysis: 3 scripts, no routes/endpoints, no source map |
| `ev_c84c7f18…2ebce92da` / `ev_f6800e41…0a194cc1` | Page inspection + script export. `app.js?v=20261009`, sha256 `92f9d3c2…5dd65d`, 42,543 B, 738 lines, unminified. Other 2 scripts: `js` class toggle + PostHog analytics |
| `ev_e6a91817…5e61bd4` | Live listeners on `#strip-demo .cym-stage`: wheel (passive:false), pointerdown/move/up/cancel → app.js L195–199 |
| `ev_404add75…5df8fb8d1` | Coverage window 1 (45 s): only hover/pointermove ran |
| `ev_8364f1a3…0bce911a` | Coverage window 2 (40 s): user keys ran. 49 keydowns (L607), 42 `act` calls, `Spring.step` ×7623, `tick` ×844 frames, `reveal` ×42, `layout` ×43 |

Line numbers below are in the exported app.js.

## How the demo's layout model works
- **Model** (static, L128–160): a strip owns `cols[]`; each column is `{wins[], frac, tabbed, row}`; `fc` = focused column index; `row` = focused window within that column. Markup is flattened into absolutely positioned windows.
- **Geometry** (L216–248): columns laid out left→right starting at `x = gap`, each `w = round(frac·(W−gap) − gap)`, next `x += w + gap`. `total = x`. Width is a fraction of the *screen*, not of remaining space. Runtime: `layout` ran 43×.
- **Stacks** (L226): `n` windows in a column split height equally: `h = (H − (n−1)·gap)/n`, `y = i·(h+gap)`.
- **Tabbed column** (L223–245): if `tabbed && n>1`, every window gets `y = tabH+5`, full remaining height; non-focused rows hidden; a tab bar (18 px, 15 narrow, 13 mini) is drawn. Toggle requires ≥2 windows (L350). *Runtime: not exercised (tabs message L29 count 0) — static only.*
- **Width presets** (L8, L325–328): `[1/4, 1/3, 1/2, 2/3, 3/4]`; cycle picks the first preset strictly greater than `frac+0.01`, wrapping to 1/4. “Full” (L330) toggles to 1.0 and back to the saved `prev`. Runtime: `width` msg ×4, `full` ×2.
- **Scroll follows focus** (L283–290): minimal scroll — if the focused column's left edge (minus gap) is left of the viewport, align it; if its right edge (plus gap) is past the viewport, align right; clamp to `[0, total−W]`. “Centre” (L366–368) sets target to column centre − W/2 (not clamped there). Runtime: `reveal` ×42.
- **Spring** (L105–113, L442–463): critically damped, exact closed-form step, retargetable (only `t` changes; `x`,`v` continue). ω = 13 for scroll, 15 for window x/y/w/h. dt capped at 34 ms. Snaps to rest when |x−t|<0.06 and |v|<0.6. One rAF loop runs only while something moves. Runtime: `Spring.step` ×7623, `tick` ×844.
- **Keyboard** (L598–635): only while the hero is hovered or contains focus; ⌘ ignored; lone ⌃ or ⌥ ignored, ⌃⌥ accepted (bare keys too). Uses `e.code`. ←/→ focus, ⇧←/→ move column, ↑/↓ (or K/J) row, H/L, R cycle width, F full, C centre, ] stack into left neighbour, [ stack into right neighbour, ⇧] / ⇧[ pop out right/left, ⇧T tabbed. Comment says real-app defaults are ⌃⌥+key (claim from the site, unverified). Runtime: focusDir ×14, move ×7, width ×4, full ×2, stack ×1, centre ×1.
- **Move column** (L319–324): swap with neighbour, focus follows. Runtime ×7.
- **Open** (L383–391): new window becomes its own column at `fc+1`, default frac 0.5, gets focus; others keep their widths. *Static only.*
- **Close** (L397–420): remove from column; drop empty column; focus moves left if needed; layout + reveal. *Static only.*
- **Trackpad/drag** (L467–540): horizontal-dominant wheel drives scroll directly with rubber-banding past edges; 130 ms after last wheel event it “settles”: projects `x + v·0.22`, snaps to nearest column left edge, and refocuses the first fully visible column if the focused one is off-screen. Title-bar drag reorders columns. *Runtime: wheel handler count 0; one click/drag (onDown/onUp ×1, focusWin ×1). Swipe/rubber-band = static only.*
- **Multi-display** (L639–661): two independent `Strip`s; “send” takes the focused window from one and inserts it into the other. Each screen keeps its own strip. *Static only.*

## Unknown / not verified
- Anything about the real macOS app (how it moves windows, AX vs private APIs, persistence, real defaults). Out of scope.
- Initial column widths of the hero demo (DOM capture redacted attribute values).
- Runtime confirmation of: tabbed columns, unstack, open/close, wheel/swipe settle, multi-display hand-off.
- “Float” (⌃⌥T) is mentioned only in a code comment; no behaviour in the demo.

## Build decisions for my app (Reel) — my own design, not from the demo
- SwiftPM executable bundled into `Reel.app` by `build.sh`; ad-hoc signed. LSUIElement menu bar app.
- Accessibility API only (AXUIElement / AXObserver). No SIP changes, no private APIs.
- Gotcha: ad-hoc signing changes the code hash each build, so macOS may require re-granting Accessibility after rebuilds.

## Milestone 2 — what I learned building it
- Hotkeys: Carbon `RegisterEventHotKey` (⌃⌥←/→). Needs no Input Monitoring permission.
- Focusing another app's window from a background menu-bar app: set the app element's
  `AXFrontmost`, the window's `AXMain`, then `AXRaise`. (`NSRunningApplication.activate`
  is unreliable for background apps on macOS 14+.)
- Coordinates: AX is top-left origin anchored to the primary display; AppKit is bottom-left.
  `ScreenGeometry` converts.
- Off-screen windows: placed at the viewport edge with a 2 pt sliver visible ("parking").
  Unknown yet: whether every app tolerates this; some may get pulled back on screen by macOS.
- Apps can refuse a width (minimum sizes). Reel reads back the accepted size and re-lays out once.
- Focus is resynced from the system-focused window before each command, so mouse clicks are respected.

## Milestone 3 — design choices (mine)
- Spring: damped harmonic oscillator parameterised by response (0.28 s) and damping ratio (1.0),
  semi-implicit Euler with 1/480 s sub-steps, retargetable (velocity kept). Different formulation
  from the demo's closed-form step; same goal (no overshoot, interruptible).
- Frame loop: 120 Hz `Timer` in common run-loop modes, runs only while something is moving.
  Only windows whose rounded x changed get an AX move per frame.
- Widths jump, positions glide: per-frame AX resizes make apps re-layout content every frame.
- Width presets: 1/3, 1/2, 2/3, full. ⌃⌥R forward, ⌃⌥⇧R back. Respects "Reduce motion".
- Unknown: real-world AX move throughput per app; heavy apps may stutter during slides.

## Milestone 4 — what I learned / chose
- Events: one `AXObserver` per regular app (window created, focused window changed) plus
  per-managed-window destroyed / minimized / deminimized. NSWorkspace launch/terminate/activate
  notifications add/remove observers and catch app switches.
- New windows can arrive before their subrole/frame is readable; Reel retries 4× at 120 ms.
- Destroyed elements may not compare equal any more, so removal falls back to "frame unreadable".
- Race: system focus notifications lag Reel's own focus changes; rapid key presses were being
  undone. Reel ignores system focus sync for 0.4 s after it moves focus itself.
- Close policy (mine): focus goes to the column that slides into the gap (right neighbour),
  else the left one. Minimizing = closing; un-minimizing = new window beside focus.
- Clicking a window (focus change) now scrolls the strip to reveal it.
- Unknown: apps whose new windows open on another display are ignored until Milestone 5.
  Manual drags/resizes by the user are overwritten on the next layout.
- Bug (found in testing): a window minimized when tiling started was never watched, so
  restoring it went unnoticed (seen with Codex). Fix: watch minimized windows at rescan, and
  adopt any unmanaged tileable window as soon as it gains focus.
- Bug: fixed-width windows (System Settings) refuse to grow; Reel only handled "too wide".
  Columns now always use the width the app accepted (min or max), and ⌃⌥R beeps if nothing changed.

## Milestone 5 — stacks, displays, swipe, persistence (my design)
Files: `Strip.swift` (pure model), `StripController.swift` (one display), `Desk.swift`
(all displays, events, hotkeys, saving), `ScrollGesture.swift`, `TabBar.swift`, `LayoutStore.swift`.

### Shortcuts
| Keys | Action |
|---|---|
| ⌃⌥ ← → | focus column left / right |
| ⌃⌥ ↑ ↓ | focus window up / down in a stack |
| ⌃⌥⇧ ← → | move column left / right |
| ⌃⌥⇧ ↑ ↓ | move window up / down in its stack |
| ⌃⌥ [ ] | stack focused window into the left / right column |
| ⌃⌥⇧ [ ] | pop window out of its stack, to the left / right |
| ⌃⌥ T | tabbed column on / off (needs ≥ 2 windows) |
| ⌃⌥ R / ⌃⌥⇧ R | cycle width 1/3 → 1/2 → 2/3 → full, forward / back |
| ⌃⌥ F | full width on / off (returns to previous width) |
| ⌃⌥ C | centre focused column |
| ⌃⌥ , . | focus display left / right |
| ⌃⌥⇧ , . | send focused window to display left / right |
| ⌃⌥ + two-finger swipe (or wheel) | scroll the strip under the pointer, coasts to a column edge |

### How it works
- Stacks split column height equally (gaps between). Tabbed columns give every window the full
  height under a 26 pt tab bar; the focused one is raised. The tab bar is Reel's own
  non-activating panel (no public API to put tabs into another app's window).
- One `StripController` per `NSScreen`. Windows go to the display containing their centre, else
  the nearest one; windows already in a strip stay put even while parked off-screen.
  Display add/remove rebuilds strips (debounced 0.6 s) from the just-saved layout.
- Parking with multiple displays: a side edge that touches another display can't be used (the
  sliver would show on the neighbour), so columns leaving through that edge drop below the
  bottom edge instead.
- Swipe: session `CGEventTap` on scroll events, only while ⌃⌥ held; events are swallowed so the
  app underneath doesn't scroll. Rubber-band past the ends (tanh), on release projects
  0.18 s of velocity and snaps to the nearest column start; focus moves to the first fully
  visible column if the focused one ended off-screen. Three-finger swipes are not possible
  without private multitouch APIs (and macOS owns them for Spaces).
- Manual resize of a tiled window → its column adopts that width.
- Persistence: `~/Library/Application Support/Reel/layout.json`, per display UUID: column order,
  stacks, widths, tabbed, focus, gap, and whether tiling was on (auto-resumes at launch). AX
  elements don't survive restarts, so windows are re-matched by bundle id + (pid or title),
  size as tie-breaker. Unmatched windows are appended by position.
- Closed-window detection uses `kAXErrorInvalidUIElement`, not "frame unreadable" (a busy app
  times out too). Each window gets a 0.3 s AX messaging timeout so a hung app can't freeze the
  animation loop.

### Unknown / untested
- Not yet tested on real hardware with 2+ displays; "park below the bottom edge" may be
  pulled back by macOS for some apps.
- Swipe direction sign assumes natural scrolling semantics of `PointDeltaAxis2`; if it feels
  inverted it's a one-character flip in `StripController.drag(by:)`.
- If "Use scroll gesture with modifier keys to zoom" is set to ⌃ in Accessibility settings,
  macOS may zoom instead of Reel receiving ⌃⌥ + scroll.
- Window re-matching after a reboot depends on titles; apps with generic titles may restore
  in a different order.
- Windows on other Spaces / native full-screen windows are ignored.

## Performance notes (Oct 21)
- Asked to rewrite in Rust for speed. Not done (yet): per-frame cost is dominated by synchronous
  cross-process AX requests (`AXUIElementSetAttributeValue` into Cursor/Chrome/Slack…), which a
  Rust port would make identically. Fixed the actual bottlenecks in place instead:
  - moves for different apps now run in parallel (`WindowMover`, `concurrentPerform` per pid);
  - frame loop driven by the screen's `CADisplayLink` (macOS 14+) instead of a 120 Hz Timer;
  - overview redrawn with Core Animation layers; only the viewport outline moves per frame
    (previously a SwiftUI model published every frame on the main thread);
  - spring response 0.28 s → 0.24 s.
- Each animation now logs `frames, moves/frame, push avg/max ms` to ~/Library/Logs/Reel/reel.log,
  so the Rust question can be answered with numbers.
- Stack/unstack now remembers each window's own width share (`shareBeforeStack`).

## Code signing (Oct 21)
- Self-signed "Reel Local Signing" code-signing identity in the login keychain (RSA 2048,
  10 years, EKU codeSigning, imported with codesign allowed via `-T /usr/bin/codesign`).
  Not marked as trusted system-wide; codesign doesn't need that.
- `build.sh` signs with it when present. Designated requirement becomes
  `identifier "local.reel.wm" and certificate leaf = H"…"`, which doesn't change between
  builds, so the Accessibility (TCC) approval survives rebuilds.
- To remove: Keychain Access → login → My Certificates → delete "Reel Local Signing".

## Width keys, login item, mouse, focus following (Oct 21)
- ⌃⌥ - / = : width in 1/12 steps (min 1/6, max full); off-grid widths snap toward the
  direction of travel first. ⌃⌥1–9 jump to column N, ⌃⌥0 to the last column.
- Launch at login: `SMAppService.mainApp` (macOS 13+), toggle in the menu; shows up in
  System Settings → General → Login Items.
- Drag to reorder: built, then removed at the user's request (keyboard only). It needed
  per-window `kAXWindowMovedNotification`s, which Reel's own animation fires every frame.
- Overview pill is clickable (non-activating panel): click a block to focus that window,
  click a tabbed block to cycle its tabs; hovering keeps it from fading.
- Focus border / dim inactive: Reel's own click-through panels at `.floating` level,
  repositioned with the windows each frame. Off by default. Limitation: being above all
  normal windows, they also sit above other apps' dialogs that overlap tiled windows.
- Following focus from Spotlight / Mission Control / Dock / ⌘Tab: previously used the
  system-wide focused window, which can still be the previous app's window right after an
  activation. Now asks the activated app (`AXFocusedWindow`, then `AXMainWindow`), also
  listens for `AXMainWindowChanged`, and retries 4× at 80 ms.

## Customizable shortcuts (Oct 21)
- `Shortcuts.swift`: `Command` (every bindable action), `KeymapPreset` (⌃⌥ arrows, ⇧ arrows,
  custom), spelled-out key names (`HotKeys.Combo.spelled`), validation (`problem`), and
  `KeymapStore` (UserDefaults: `keymapPreset`, `customBindings` as JSON).
- Choosing Customize copies the active preset into the custom map first. Assigning a combo
  already in use unassigns the other command and says which.
- Recording suspends Reel's global shortcuts (Carbon hotkeys and the event-tap fallback would
  otherwise eat the keys before the editor sees them).
- Overview no longer pops up when focus changes from outside Reel (mouse click, Spotlight,
  Dock); it still updates silently and shows after keyboard commands and swipes.
- README.md is the user-facing documentation; NOTES.md stays the engineering log.

## Scrolling rework (Oct 21)
- Log showed swipes cost ~1 ms/frame, so the jank wasn't AX throughput. Causes: windows were
  moved synchronously inside the event-tap callback for every scroll event (uneven event
  timing → stutter, and it delays the next event), and 1:1 point mapping felt slow.
- Now scroll events only move the spring target; the display-link loop moves windows once
  per refresh, with the spring stiffened to 0.06 s response while fingers are down
  (back to 0.24 s on release, keeping momentum). Projection 0.2 s of release velocity.
- Settings: scroll modifier (⌃⌥ / ⇧ / ⌥ / ⌘ / ⌃, matched exactly) and speed 1–4× (default 2×).

## Robustness after a force quit (Oct 21)
- Incident: shortcut editor put into full screen → lost its Space, hid behind windows; Reel was
  force-quit (SIGKILL) while tiling, leaving parked windows off screen until Reel relaunched.
- Editor window: `.fullScreenNone`; activation policy switches to `.regular` while it's open
  (Dock icon + ⌘Tab) and back to `.accessory` on close.
- SIGTERM/SIGINT/SIGHUP handled via DispatchSource: stop tiling (restores windows), then exit.
  SIGKILL can't be handled by any app.
- At launch with tiling off, and via a menu button, `Desk.rescueOffscreenWindows()` moves any
  standard window with < 100×100 pt visible on every display back onto the main display.

## Rust rewrite (Oct 21)
- Swift version moved to `swift-version/` (source + a packaged copy of its app). The Rust app
  (`src/`, objc2 bindings, native NSMenu instead of a SwiftUI popover) reads the same
  NSUserDefaults keys and `layout.json`, so settings and custom shortcuts carried over.
- Builds need only Rust + Command Line Tools (no Xcode). Release binary ≈ 0.9 MB.
- Design changes over Swift, for responsiveness:
  - window moves go to one worker thread per app that always applies the *latest* position
    (`mover.rs`); a slow app lags alone, the main thread never waits. Main-thread work per
    frame fell from ~1–4 ms (Swift) to ~0.07 ms in the user's log.
  - the scroll event tap and the keyboard fallback tap run on their own threads, so a busy
    Reel can never stall typing or scrolling system-wide.
  - one display-link clock for all strips; every path that starts an animation starts it.
- End-to-end harness (still Swift: ReelTestHost + ReelTestRunner) runs a separate test-mode
  Reel against dummy windows and checks real frames. Final run: 109/109, nothing left open.
- Bugs it found (the first four also exist in the Swift version, which is not patched):
  1. apps nudge window height/position slightly after activation or while parked; Reel
     trusted its cache. Now re-verifies after each animation and after focusing (0.35 s,
     1.2 s), and restores height whenever the app changes it by itself.
  2. quitting could leave windows parked: a stale queued move landed after restore. Now
     moves are flushed before restoring.
  3. focus-following adopted windows from apps Reel must ignore (e.g. test windows, or
     other apps during a test run). Now checks `apps::manages_pid`.
  4. windows of a quit app lingered as ghost columns (AX reports a different error for a
     dead process). Now: process liveness check + remove all windows on app quit.
  5. (Rust port) animations started by window events never started the frame clock.
- Test-runner fixes: always closes host, test Reel and watchdog (end, failure, interrupt);
  clears leftovers before starting; recovery check no longer races the ready file.

## Swift panel in the Rust app (Oct 22)
- Tried switching the published app back to Swift (0.3.0), then reverted and removed it from
  history. The Swift version with the ported fixes stays in `../reel-swift`.
- Instead, the Rust app now uses the Swift version's SwiftUI menu bar panel: `swift/Panel.swift`
  is compiled by `build.rs` (`xcrun swiftc -emit-library -static`) and linked into the Rust
  binary. Only `swiftc` is needed (Command Line Tools), not SwiftPM or Xcode.
- Interface (C ABI, main thread only): `reel_panel_install(state, free, action)`,
  `reel_panel_close()`. `state()` returns the panel's state as JSON (`src/panel.rs`);
  `action(name, value)` carries out a change. The panel re-reads the state every 0.5 s while
  open, and right after each action. If the desk is busy, the last state is reused.
- Linking notes: Swift availability checks need `___isPlatformVersionAtLeast` from
  `libclang_rt.osx.a`; Rust's deployment target is set to 13.0 (`.cargo/config.toml`) to
  match. The Swift runtime comes from the OS (`/usr/lib/swift`).
- Native NSMenu (`menu.rs`) and its MenuDelegate class removed. Column rows in the panel are
  clickable (kept from the Rust menu). The shortcut editor stays the Rust/AppKit one.
- Verified: clippy clean, 25 unit tests, 111/111 end-to-end, panel rendered via
  `--render-panel` in both states.

## Animation work (Oct 22, local only, not committed)
- Springs now match the web demo: critically damped, ω 13 for the strip's scroll, ω 15 for
  columns (were one 0.24 s spring, about 2× faster). New **Animation speed** slider (0.5–2.5×).
- Reel's own per-frame work was already ~0.07 ms; the choppiness is other apps applying AX
  moves late. Hence the experiments below, each an independent switch, off by default, in the
  panel's "Animation experiments" section (`src/experiments.rs`, saved as `exp.<key>`):
  - Live resizing: width changes animate on the real window (size sent with each move).
  - Pause enhanced UI: `AXEnhancedUserInterface` (undocumented) off while moving, restored after.
  - Focus after the glide: the focus/raise waits until the move settles.
  - High-priority move threads: mover threads at QOS_CLASS_USER_INTERACTIVE.
  - Eased curve: cubic Hermite over a fixed duration (0.42 s at 1×), keeps velocity on retarget.
  - Softer swipe tracking: finger-follow spring response 0.13 s instead of 0.06 s.
  - Steady 60 Hz: real windows get positions at most every 15 ms; final frame always sent.
- Test mode: `exp:<key>:on|off`; a runner scenario turns them all on and checks layouts.
- Removed: snapshot moves/swipes/resizing (ScreenCaptureKit pictures in an overlay while the
  real windows waited below the screen). Capturing took 40–120 ms before each move, the user
  saw no improvement, and quick key presses made windows flicker: the next move dropped the
  pictures before the real windows were back in place.
- Testing caveat: with the screen locked, Accessibility sees no windows (the runner now says
  so and stops) and codesign can't reach the keychain.

## 0.4.0 (Oct 25)
- One built-in keymap: ⇧ arrows (focus) / ⌥⇧ arrows (move) / ⌃⌥ for everything else. The
  ⌃⌥-arrows layout was removed; a saved `keymapPreset = ctrlOpt` maps to it. Custom stays.
- ⌃⌥ -/= step through ⅙ ¼ ⅓ ½ ⅔ ¾ ⅚ full (no wrap) instead of 1/12 nudges.
- A growing window near the right edge slides left first: macOS caps a window's width to
  the room on its right, which made the last column barely grow.
- Display changes while the screen is locked wait for a real unlock (Accessibility sees no
  windows while locked). Sizes an app chooses itself (Terminal's row-snapped heights) are
  accepted instead of re-corrected after every move.
- "Animate window moves" switch (off = instant moves); reopening Reel shows its panel.
