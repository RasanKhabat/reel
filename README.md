# Reel

A scrolling window manager for macOS. Your windows sit side by side on a horizontal
strip that is wider than the screen; moving focus slides the strip so the focused window
is always fully visible. Nothing gets squeezed to make room for a new window: it simply
opens next to the one you're using.

Reel runs in the menu bar, moves other apps' windows through the macOS Accessibility API,
and needs no changes to System Integrity Protection.

## Install

**Requirements:** macOS 13 Ventura or later on an Apple silicon Mac.

### With Terminal

Paste this into Terminal. It downloads the latest release, puts Reel.app in Applications and
opens it:

```bash
curl -fL -o /tmp/Reel.zip https://github.com/RasanKhabat/reel/releases/latest/download/Reel-macos-arm64.zip
ditto -x -k /tmp/Reel.zip /Applications && rm /tmp/Reel.zip
open /Applications/Reel.app
```

Then continue at step 4 below. (Downloads made with `curl` aren't marked as coming from the
internet, so macOS doesn't ask you to approve the app first.)

To update later, quit Reel from its panel and run the same three lines again. macOS treats
each new version as a different app, so turn Reel off and on again under
**Privacy & Security → Accessibility** if it asks for access.

### With Homebrew

```bash
brew install --cask rasankhabat/reel/reel
```

Then open Reel from Applications and continue at step 3 below (Homebrew marks its downloads as
coming from the internet, so macOS asks you to approve the app once). Update with
`brew upgrade --cask reel`; remove with `brew uninstall --cask reel` (add `--zap` to also
remove settings and logs). Quit Reel from its panel before either.

### With your browser

1. Download `Reel-<version>-macos-arm64.zip` from the
   [latest release](https://github.com/RasanKhabat/reel/releases/latest) and unzip it.
2. Move **Reel.app** to your **Applications** folder.
3. Open it. Reel isn't signed with a paid Apple Developer ID, so the first time macOS says it
   can't verify the app. Open **System Settings → Privacy & Security**, scroll down, and click
   **Open Anyway** next to the message about Reel (you only do this once).
   On older macOS versions you can instead right-click Reel.app → **Open** → **Open**.
4. macOS then asks to allow Reel under **Privacy & Security → Accessibility**. Turn it on;
   Reel needs this to move and resize other apps' windows, and notices within a second.
5. Click the Reel icon in the menu bar (three side-by-side rectangles) and turn on
   **Tile windows**. Tick **Launch at login** in the same panel to start Reel automatically.

Prefer to check what you run? Each release lists a SHA-256 checksum
(`shasum -a 256 -c Reel-<version>-macos-arm64.zip.sha256` in the download folder), or build it
yourself (below).

### Uninstall

Quit Reel from its panel (this puts all windows back on screen), untick **Launch at login**
if you enabled it, delete **Reel.app**, and optionally remove its settings:

```bash
rm -rf ~/Library/Application\ Support/Reel ~/Library/Logs/Reel
defaults delete local.reel.wm
```

Then remove Reel from **Privacy & Security → Accessibility**.

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

## Key names

Shortcuts are shown with the standard Mac symbols:

| Symbol | Key |
|---|---|
| ⌃ | Control |
| ⌥ | Option |
| ⇧ | Shift |
| ⌘ | Command |
| ← → ↑ ↓ | Left, Right, Up, Down arrow |

## Shortcuts

Shortcuts only work while **Tile windows** is on. Choose a layout in the Reel panel under
**Shortcuts**: **⌃⌥ arrows**, **⇧ arrows**, or **Customize** to set your own.

The two built-in layouts differ only in the arrow keys:

| Action | ⌃⌥ arrows layout | ⇧ arrows layout |
|---|---|---|
| Focus column to the left / right | ⌃ Control + ⌥ Option + ← Left / → Right | ⇧ Shift + ← Left / → Right |
| Focus window above / below in a stack | ⌃ Control + ⌥ Option + ↑ Up / ↓ Down | ⇧ Shift + ↑ Up / ↓ Down |
| Move column left / right | ⌃ Control + ⌥ Option + ⇧ Shift + ← Left / → Right | ⌥ Option + ⇧ Shift + ← Left / → Right |
| Move window up / down in its stack | ⌃ Control + ⌥ Option + ⇧ Shift + ↑ Up / ↓ Down | ⌥ Option + ⇧ Shift + ↑ Up / ↓ Down |

With the ⇧ arrows layout, ⇧ Shift + arrow can't be used to select text in other apps while
tiling is on.

Everything else is the same in both layouts:

| Action | Shortcut |
|---|---|
| Stack window into the left column | ⌃ Control + ⌥ Option + [ Left Bracket |
| Stack window into the right column | ⌃ Control + ⌥ Option + ] Right Bracket |
| Pop window out of its stack, to the left | ⌃ Control + ⌥ Option + ⇧ Shift + [ Left Bracket |
| Pop window out of its stack, to the right | ⌃ Control + ⌥ Option + ⇧ Shift + ] Right Bracket |
| Tabbed column on / off | ⌃ Control + ⌥ Option + T |
| Unstack all (every window its own column) | ⌃ Control + ⌥ Option + U |
| Cycle width forward (⅓ → ½ → ⅔ → full) | ⌃ Control + ⌥ Option + R |
| Cycle width back | ⌃ Control + ⌥ Option + ⇧ Shift + R |
| Next size down / up (⅙ ¼ ⅓ ½ ⅔ ¾ ⅚ full) | ⌃ Control + ⌥ Option + - Minus / = Equals |
| Full width on / off | ⌃ Control + ⌥ Option + F |
| Centre focused column | ⌃ Control + ⌥ Option + C |
| Jump to column 1 … 9 | ⌃ Control + ⌥ Option + 1 … 9 |
| Jump to last column | ⌃ Control + ⌥ Option + 0 |
| Focus display to the left / right | ⌃ Control + ⌥ Option + , Comma / . Period |
| Send window to display on the left / right | ⌃ Control + ⌥ Option + ⇧ Shift + , Comma / . Period |
| Scroll the strip under the pointer | ⌃ Control + ⌥ Option + two-finger swipe (or mouse wheel); the modifier can be changed, see below |

### Scrolling the strip

Hold the scroll modifier and swipe two fingers sideways on the trackpad (or turn the mouse
wheel) to slide the strip under the pointer; let go and it glides to the nearest column.
In the Reel panel:

- **Scroll with** picks the modifier: ⌃⌥ (⌃ Control + ⌥ Option, the default), ⇧ Shift,
  ⌥ Option, ⌘ Command or ⌃ Control. While tiling is on, Reel takes over that combination,
  e.g. with ⇧ Shift, Shift + scroll no longer scrolls sideways inside apps.
- **Scroll speed** (1× to 4×, default 2×) sets how far the strip moves per swipe.

### Your own shortcuts

In the Reel panel pick **Shortcuts → Customize** (afterwards, **Edit shortcuts…** reopens it).
In the window that opens, click an action and press the keys you want:

- **Esc** cancels, **⌫ Delete** removes the action's shortcut.
- A key combination can only do one thing; giving it to a second action takes it away from
  the first, and Reel tells you which.
- Shortcuts must include ⌃ Control, ⌥ Option or ⌘ Command, so normal typing keeps working.
  ⇧ Shift on its own is allowed only with keys that don't type anything (arrows, F-keys,
  Home/End, Page Up/Down).
- **Reset everything to** puts back either built-in layout.

**Keyboard shortcuts** in the Reel panel always lists the shortcuts that are currently active,
with every key spelled out.

## What else it does

- **New windows** open as a new column right of the one you're using; closing a window
  closes the gap.
- **Following focus**: picking a window through Spotlight, Mission Control, the Dock or
  ⌘ Command + Tab scrolls the strip to it.
- **Stacks and tabs**: several windows can share a column, split top to bottom or as tabs.
- **Overview**: a small map above the Dock shows every column, the focused window and which
  part of the strip is on screen. Click a block to jump there. Choose when it appears
  (*On change*, *Always*, *Off*) in the panel; clicking a column in the panel's list works too.
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

## Development

- `cargo test` runs the unit tests (layout model, spring, saved layouts, shortcuts);
  `cargo clippy` should report nothing.
- GitHub Actions runs both on every push (`.github/workflows/ci.yml`). Pushing a tag such as
  `v0.2.0` builds the app and publishes it as a GitHub Release (`release.yml`).
- Reel has a test mode (`--test-pid <pid> --test-dir <dir>`) in which it manages only one
  process's windows and takes commands over a distributed notification; end-to-end tests use
  it to drive Reel against dummy windows without touching real ones.
- The menu bar panel is SwiftUI (`swift/Panel.swift`); `build.rs` compiles it with `swiftc` into
  a static library linked into the Rust binary. It only displays state and sends actions;
  `src/panel.rs` is the Rust side. `Reel --render-panel out.png [--tiling]` draws it to an image.
- Diagnostics: `~/Library/Logs/Reel/reel.log` (**Open log** in the panel).

## License

MIT, see [LICENSE](LICENSE).
