# Reel

A scrolling window manager for macOS. Your windows sit side by side on a horizontal
strip that is wider than the screen; moving focus slides the strip so the focused window
is always fully visible. Nothing gets squeezed to make room for a new window: it simply
opens next to the one you're using.

Reel runs in the menu bar, moves other apps' windows through the macOS Accessibility API,
and needs no changes to System Integrity Protection.

## Install

**Requirements:** macOS 13 Ventura or later on an Apple silicon Mac (M1 or newer).

There are three ways to get Reel. They all install the same app; pick whichever suits you:

| Way | Good if… | Updating |
|---|---|---|
| [Homebrew](#with-homebrew) | you already use Homebrew | `brew upgrade --cask reel` |
| [Terminal](#with-terminal) | you're happy pasting three lines into Terminal (no Homebrew needed) | run the same three lines again |
| [Browser](#with-your-browser) | you'd rather not use Terminal at all | download the new zip |

### With Homebrew

```bash
brew install --cask rasankhabat/reel/reel
```

Then open **Reel** from your Applications folder and follow [First launch](#first-launch)
from step 1.

### With Terminal

Paste these three lines into Terminal (Applications → Utilities → Terminal). They download the
latest version, put **Reel.app** in your Applications folder and open it:

```bash
curl -fL -o /tmp/Reel.zip https://github.com/RasanKhabat/reel/releases/latest/download/Reel-macos-arm64.zip
ditto -x -k /tmp/Reel.zip /Applications && rm /tmp/Reel.zip
open /Applications/Reel.app
```

Then follow [First launch](#first-launch) from step 2. (macOS doesn't ask you to approve apps
downloaded this way, so step 1 doesn't apply.)

### With your browser

1. Open the [latest release](https://github.com/RasanKhabat/reel/releases/latest) and download
   `Reel-<version>-macos-arm64.zip`.
2. Double-click the zip to unzip it, and drag **Reel.app** into your **Applications** folder.
3. Open it, then follow [First launch](#first-launch) from step 1.

### First launch

1. **Allow the app (Homebrew and browser installs only, once).** Reel isn't signed with a paid
   Apple Developer ID, so macOS first says it can't verify the app. Open
   **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next to the
   message about Reel. (On older macOS: right-click Reel.app → **Open** → **Open**.)
2. **Allow Accessibility.** macOS asks to allow Reel under
   **Privacy & Security → Accessibility**. Turn it on: Reel needs it to move and resize other
   apps' windows. Reel notices within a second; no restart needed.
3. **Turn tiling on.** Click the Reel icon in the menu bar (three side-by-side rectangles) and
   switch on **Tile windows**. Your windows line up side by side.
4. **Optional:** tick **Launch at login** at the top of the same panel.

Can't find the icon? On a MacBook with a notch, crowded menu bar icons hide behind it. Open
Reel again from Applications or Spotlight and its panel drops down.

### Updating

- **Homebrew:** quit Reel from its panel, then `brew upgrade --cask reel`.
- **Terminal:** quit Reel from its panel, then run the same three lines again.
- **Browser:** quit Reel, download the new zip and replace Reel.app in Applications.

macOS treats each new version as a different app, so if Reel asks for access again, turn it
off and on under **Privacy & Security → Accessibility**.

Want to check what you run? Each release lists a SHA-256 checksum
(`shasum -a 256 -c Reel-<version>-macos-arm64.zip.sha256` in the download folder), or
[build it yourself](#build-from-source).

### Uninstall

Quit Reel from its panel (this puts all windows back on screen) and untick **Launch at login**
if you enabled it. Then:

- **Homebrew:** `brew uninstall --cask reel` (add `--zap` to also remove its settings and logs).
- **Otherwise:** delete **Reel.app**, and optionally its settings and logs:

```bash
rm -rf ~/Library/Application\ Support/Reel ~/Library/Logs/Reel
defaults delete local.reel.wm
```

Finally remove Reel from **Privacy & Security → Accessibility**.

## Keyboard shortcuts

Shortcuts work while **Tile windows** is on.

### The keys

Mac shortcuts are written with these symbols:

| Symbol | Key | Where it is |
|---|---|---|
| ⇧ | Shift | the large keys either side, above Control/Fn |
| ⌃ | Control | bottom row, left (marked *control*) |
| ⌥ | Option | next to Command (marked *option* or *alt*) |
| ⌘ | Command | either side of the space bar |
| ← → ↑ ↓ | arrow keys | bottom right |

A combination like **⌃⌥ R** means: hold ⌃ Control and ⌥ Option together, then press R.

### How they fit together

You only need to remember four rules:

1. **⇧ Shift + arrow** moves the *focus*: to the column on the left or right, or to the window
   above or below in a stack.
2. **⌥ Option + ⇧ Shift + arrow** moves the *window itself* the same way, taking it with you.
3. **⌃ Control + ⌥ Option + a key** does everything else: widths, stacks, tabs, jumping,
   displays.
4. **Add ⇧ Shift to a ⌃⌥ shortcut to go the other way:** ⌃⌥ [ stacks a window into the column on
   its left, ⌃⌥⇧ [ pops it back out; ⌃⌥ R cycles widths up, ⌃⌥⇧ R cycles them down; ⌃⌥ , focuses
   the display on the left, ⌃⌥⇧ , sends the window there.

While tiling is on, ⇧ Shift + arrows belong to Reel, so they don't select text in other apps.
If you need that, give those actions other keys (see [Your own shortcuts](#your-own-shortcuts)).

### All shortcuts

**Moving around**

| Action | Shortcut |
|---|---|
| Focus the column to the left / right | ⇧ Shift + ← / → |
| Focus the window above / below (in a stack) | ⇧ Shift + ↑ / ↓ |
| Move the column left / right | ⌥ Option + ⇧ Shift + ← / → |
| Move the window up / down in its stack | ⌥ Option + ⇧ Shift + ↑ / ↓ |
| Jump to column 1 … 9 | ⌃ Control + ⌥ Option + 1 … 9 |
| Jump to the last column | ⌃ Control + ⌥ Option + 0 |
| Centre the focused column on screen | ⌃ Control + ⌥ Option + C |
| Show the desktop (press again, or switch to a window, to bring them back) | ⌃ Control + ⌥ Option + D |

**Width**

| Action | Shortcut |
|---|---|
| Next size up / down (⅙ ¼ ⅓ ½ ⅔ ¾ ⅚ full; stops at the ends) | ⌃ Control + ⌥ Option + = / - |
| Cycle ⅓ → ½ → ⅔ → full (and round again) | ⌃ Control + ⌥ Option + R |
| Cycle the other way | ⌃ Control + ⌥ Option + ⇧ Shift + R |
| Full width on / off | ⌃ Control + ⌥ Option + F |

**Stacks and tabs** (several windows sharing one column)

| Action | Shortcut |
|---|---|
| Stack the window into the column on the left / right | ⌃ Control + ⌥ Option + [ / ] |
| Pop the window out of its stack, to the left / right | ⌃ Control + ⌥ Option + ⇧ Shift + [ / ] |
| Show the stack as tabs, or split again | ⌃ Control + ⌥ Option + T |
| Unstack everything (every window its own column) | ⌃ Control + ⌥ Option + U |

**More than one display**

| Action | Shortcut |
|---|---|
| Focus the display on the left / right | ⌃ Control + ⌥ Option + , / . |
| Send the window to the display on the left / right | ⌃ Control + ⌥ Option + ⇧ Shift + , / . |

The **Keyboard shortcuts** section of the Reel panel always lists the shortcuts in use, with
every key spelled out.

### Scrolling the strip

Hold the scroll modifier and swipe two fingers sideways on the trackpad (or turn the mouse
wheel) to slide the strip under the pointer; let go and it settles on the nearest column.
In the Reel panel:

- **Scroll with** picks the modifier: ⌃⌥ (⌃ Control + ⌥ Option, the default), ⇧ Shift,
  ⌥ Option, ⌘ Command or ⌃ Control. While tiling is on, Reel takes over that combination;
  for example with ⇧ Shift, Shift + scroll no longer scrolls sideways inside apps.
- **Scroll speed** (1× to 4×, default 2×) sets how far the strip moves per swipe.

### Your own shortcuts

In the Reel panel choose **Shortcuts → Customize** (afterwards **Edit shortcuts…** reopens the
editor). Click an action and press the keys you want:

- **Esc** cancels, **⌫ Delete** removes that action's shortcut.
- A combination can only do one thing; giving it to a second action takes it from the first,
  and Reel tells you which.
- Shortcuts need ⌃ Control, ⌥ Option or ⌘ Command so normal typing keeps working. ⇧ Shift on
  its own is allowed only with keys that don't type anything (arrows, F-keys, Home/End,
  Page Up/Down).
- **Reset to the default shortcuts** puts back the layout above.

Switching back to **⇧ arrows** in the panel uses the default layout again; your own
shortcuts are kept for next time you choose Customize.

## Animation

- **Animate window moves** (on by default): windows glide to their new places. Off, every move
  is instant. Reel also skips animations when macOS's Reduce Motion setting is on.
- **Animation speed** (0.5× to 2.5×): how quickly they glide.
- **Animation experiments**: optional tweaks, all off by default (for example *Live resizing*,
  which animates width changes instead of jumping).

## What else it does

- **New windows** open as a new column right of the one you're using; closing a window
  closes the gap.
- **Following focus**: picking a window through Spotlight, Mission Control, the Dock or
  ⌘ Command + Tab scrolls the strip to it.
- **Stacks and tabs**: several windows can share a column, split top to bottom or as tabs.
- **Show desktop** (⌃⌥ D, or the button in the panel): your windows slide off to the screen
  edges so you can use the desktop; press it again, use any Reel shortcut or switch to a
  window to bring them back.
- **Overview**: a small map above the Dock shows every column, the focused window and which
  part of the strip is on screen. Click a block to jump there. Choose when it appears
  (*On change*, *Always*, *Off*) in the panel.
- **Focus border / Dim inactive**: optional highlights, in the panel.
- **One strip per display.**
- **Remembers your layout** (order, stacks, widths) across restarts, in
  `~/Library/Application Support/Reel/layout.json`.
- **Launch at login**: checkbox at the top of the panel.

## Troubleshooting

- **Nothing moves**: check Reel is allowed under Privacy & Security → Accessibility and that
  **Tile windows** is on.
- **A shortcut does nothing**: the panel lists any shortcut macOS refused; a diagnostic log is
  at `~/Library/Logs/Reel/reel.log` (**Open log** in the panel).
- **Windows stuck off screen** (e.g. after Reel was force-quit): just open Reel again. With
  tiling on it picks them back up; with tiling off it moves them back on screen at launch, and
  **Bring back off-screen windows** in the panel (with tiling off) does it any time. Quitting
  Reel normally (panel → Quit, or Quit in Activity Monitor) always brings windows back first; only
  *Force Quit* can't, because macOS gives the app no chance to clean up.
- **An app won't resize**: some apps have fixed or minimum sizes. Reel follows what the app
  accepts and beeps if a width change was impossible.

## Build from source

You need [Rust](https://rustup.rs) and Apple's Command Line Tools (they include `swiftc`, which
builds the menu bar panel). Xcode is **not** needed.

```bash
xcode-select --install                                           # Command Line Tools (once)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust (once)
git clone https://github.com/RasanKhabat/reel.git
cd reel
./build.sh            # builds build/Reel.app
open build/Reel.app
```

`./build.sh` signs the app ad-hoc, which means macOS asks for Accessibility again after every
rebuild. To avoid that, create a self-signed code-signing certificate named
**Reel Local Signing** in Keychain Access (Certificate Assistant → Create a Certificate,
type *Code Signing*); `build.sh` uses it automatically when it exists.

`./release.sh` builds the downloadable zip into `dist/`.

## Development

- `cargo test` runs the unit tests (layout model, spring, saved layouts, shortcuts);
  `cargo clippy` should report nothing.
- GitHub Actions runs both on every push (`.github/workflows/ci.yml`). Pushing a version tag
  (e.g. `v0.3.1`) builds the app, publishes it as a GitHub Release and updates the Homebrew
  tap (`release.yml`).
- Reel has a test mode (`--test-pid <pid> --test-dir <dir>`) in which it manages only one
  process's windows and takes commands over a distributed notification; end-to-end tests use
  it to drive Reel against dummy windows without touching real ones.
- The menu bar panel is SwiftUI (`swift/Panel.swift`); `build.rs` compiles it with `swiftc` into
  a static library linked into the Rust binary. It only displays state and sends actions;
  `src/panel.rs` is the Rust side. `Reel --render-panel out.png [--tiling]` draws it to an image.
- Diagnostics: `~/Library/Logs/Reel/reel.log` (**Open log** in the panel).

## License

MIT, see [LICENSE](LICENSE).
