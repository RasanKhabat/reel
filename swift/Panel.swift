// The menu bar panel, in SwiftUI. Compiled by build.rs into a static library that the
// Rust app links. It holds no logic of its own: it asks Rust for the current state (as
// JSON) and sends every change back to Rust as an action.
//
// A plain NSStatusItem + fixed-size NSPopover is used instead of SwiftUI's MenuBarExtra:
// MenuBarExtra's window re-sizes itself whenever its content changes, and a shortcut
// firing while it was open left it half-drawn and unresponsive.

import AppKit
import SwiftUI

public typealias ReelStateFn = @convention(c) () -> UnsafeMutablePointer<CChar>?
public typealias ReelFreeFn = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
public typealias ReelActionFn = @convention(c) (UnsafePointer<CChar>, UnsafePointer<CChar>) -> Void

/// Called once by Rust on the main thread.
@_cdecl("reel_panel_install")
public func reelPanelInstall(_ state: ReelStateFn, _ free: ReelFreeFn, _ action: ReelActionFn) {
    MainActor.assumeIsolated {
        PanelController.shared = PanelController(bridge: Bridge(state: state, free: free, action: action))
    }
}

/// Closes the panel if it's open (a shortcut fired while it was showing).
@_cdecl("reel_panel_close")
public func reelPanelClose() {
    MainActor.assumeIsolated { PanelController.shared?.close() }
}

/// Opens the panel (Reel was launched again while already running).
@_cdecl("reel_panel_show")
public func reelPanelShow() {
    MainActor.assumeIsolated { PanelController.shared?.show() }
}

// MARK: Bridge to Rust

struct Bridge {
    let state: ReelStateFn
    let free: ReelFreeFn
    let action: ReelActionFn

    func read() -> PanelState? {
        guard let raw = state() else { return nil }
        defer { free(raw) }
        return try? JSONDecoder().decode(PanelState.self, from: Data(bytes: raw, count: strlen(raw)))
    }

    func send(_ name: String, _ value: String = "") {
        name.withCString { n in value.withCString { v in action(n, v) } }
    }
}

/// Everything the panel shows, as produced by Rust.
struct PanelState: Decodable {
    struct Item: Decodable { let title: String; let keys: String }
    struct Group: Decodable { let name: String; let items: [Item] }
    struct Experiment: Decodable {
        let key: String; let title: String; let detail: String
        let on: Bool
    }
    struct Win: Decodable {
        let title: String; let app: String
        let x: Double; let y: Double; let w: Double; let h: Double
        let standard: Bool; let minimized: Bool
    }

    var trusted = false
    var tiling = false
    var launchAtLogin = false
    var swipeAvailable = true
    var hasStacks = false
    var unstackKeys: String? = nil
    var preset = "shift"
    var scrollModifier = "ctrlOpt"
    var scrollSpelled = ""
    var scrollCaveat: String? = nil
    var scrollSpeed = 2.0
    var animationSpeed = 1.0
    var animations = true
    var experiments: [Experiment] = []
    var overview = "auto"
    var border = false
    var dim = false
    var gap = 8.0
    var failed: [String] = []
    var legend = ""
    var groups: [Group] = []
    var windows: [Win] = []
}

// MARK: Status item + popover

@MainActor
final class PanelController: NSObject {
    static var shared: PanelController?

    private let model: PanelModel
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let popover = NSPopover()
    private var timer: Timer?

    init(bridge: Bridge) {
        model = PanelModel(bridge: bridge)
        super.init()
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "rectangle.split.3x1", accessibilityDescription: "Reel")
            button.target = self
            button.action = #selector(toggle)
        }
        let host = NSHostingController(rootView: PanelView(model: model))
        host.sizingOptions = []  // fixed size; never resize while open
        popover.contentViewController = host
        popover.contentSize = PanelView.size
        popover.behavior = .transient
        popover.delegate = self
        model.close = { [weak self] in self?.close() }
        // Opening Reel again (Finder, Spotlight, `open`) while it runs shows the panel, since
        // a menu bar icon is easy to lose behind the notch. AppKit installs its own handler for
        // this event when launching finishes, so register again after that.
        installReopenHandler()
        NotificationCenter.default.addObserver(
            forName: NSApplication.didFinishLaunchingNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.installReopenHandler() }
        }
    }

    private func installReopenHandler() {
        NSAppleEventManager.shared().setEventHandler(
            self, andSelector: #selector(reopen(_:reply:)),
            forEventClass: AEEventClass(kCoreEventClass), andEventID: AEEventID(kAEReopenApplication))
    }

    @objc private func reopen(_ event: NSAppleEventDescriptor, reply: NSAppleEventDescriptor) {
        NSLog("Reel: reopened; showing the panel")
        show()
    }

    @objc private func toggle() {
        popover.isShown ? close() : show()
    }

    func show() {
        guard !popover.isShown, let button = statusItem.button else { return }
        NSApp.activate(ignoringOtherApps: true)
        model.bridge.send("refreshWindows")
        model.reload()
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
        // Keep the column list live while the panel is open.
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.model.reload() }
        }
    }

    func close() {
        if popover.isShown { popover.performClose(nil) }
    }
}

extension PanelController: NSPopoverDelegate {
    nonisolated func popoverDidClose(_ notification: Notification) {
        MainActor.assumeIsolated {
            timer?.invalidate()
            timer = nil
        }
    }
}

// MARK: Model

@MainActor
final class PanelModel: ObservableObject {
    @Published private(set) var s = PanelState()
    let bridge: Bridge
    var close: (() -> Void)?

    init(bridge: Bridge) {
        self.bridge = bridge
    }

    func reload() {
        if let next = bridge.read() { s = next }
    }

    /// Sends an action, then shows its result. Rust may run it a moment later (if it was
    /// busy), so read again on the next turn of the run loop too.
    func send(_ name: String, _ value: String = "") {
        bridge.send(name, value)
        reload()
        DispatchQueue.main.async { [weak self] in self?.reload() }
    }

    func binding(_ get: @escaping (PanelState) -> Bool, _ action: String) -> Binding<Bool> {
        Binding(get: { get(self.s) }, set: { self.send(action, $0 ? "1" : "0") })
    }

    func choice(_ get: @escaping (PanelState) -> String, _ action: String) -> Binding<String> {
        Binding(get: { get(self.s) }, set: { self.send(action, $0) })
    }
}

// MARK: View

struct PanelView: View {
    static let size = NSSize(width: 400, height: 520)

    static let presets = [("shift", "⇧ arrows"), ("custom", "Customize")]
    static let modifiers = [("ctrlOpt", "⌃⌥"), ("shift", "⇧"), ("option", "⌥"), ("command", "⌘"), ("control", "⌃")]
    static let overviews = [("auto", "On change"), ("always", "Always"), ("off", "Off")]

    @ObservedObject var model: PanelModel
    @State private var showShortcuts = false
    @State private var showExperiments = false

    private var s: PanelState { model.s }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Reel").font(.headline)
                Spacer()
                Toggle("Launch at login", isOn: model.binding({ $0.launchAtLogin }, "launchAtLogin"))
                    .toggleStyle(.checkbox).font(.caption)
            }
            if s.trusted {
                Toggle("Tile windows", isOn: model.binding({ $0.tiling }, "tiling"))
                    .toggleStyle(.switch)
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        if s.tiling {
                            settingsSection
                            if s.hasStacks {
                                Button("Unstack all" + (s.unstackKeys.map { " (\($0))" } ?? "")) { model.send("unstackAll") }
                            }
                        } else {
                            Button("Bring back off-screen windows") { model.send("rescue") }
                                .help("Moves any window stuck outside the screen back into view")
                            windowList
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                permissionPrompt
                Spacer()
            }
            Divider()
            HStack {
                Button(s.tiling ? "Re-scan windows" : "Refresh") { model.send("refresh") }.disabled(!s.trusted)
                Button("Open log") { model.send("openLog") }
                Spacer()
                Button("Quit") { model.send("quit") }
            }
        }
        .padding(12)
        .frame(width: Self.size.width, height: Self.size.height, alignment: .topLeading)
    }

    @ViewBuilder private var settingsSection: some View {
        Picker("Shortcuts", selection: model.choice({ $0.preset }, "preset")) {
            ForEach(Self.presets, id: \.0) { Text($0.1).tag($0.0) }
        }
        .pickerStyle(.segmented)
        if s.preset == "shift" {
            Text("While tiling is on, ⇧ Shift + arrows won't select text in other apps.")
                .font(.caption2).foregroundStyle(.secondary)
        } else if s.preset == "custom" {
            Button("Edit shortcuts…") { model.close?(); model.send("editShortcuts") }
        }
        Picker("Scroll with", selection: model.choice({ $0.scrollModifier }, "scrollModifier")) {
            ForEach(Self.modifiers, id: \.0) { Text($0.1).tag($0.0) }
        }
        .pickerStyle(.segmented)
        Text("Hold \(s.scrollSpelled) and swipe or scroll to move the strip." + (s.scrollCaveat.map { " " + $0 } ?? ""))
            .font(.caption2).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        HStack {
            Text("Scroll speed").font(.caption)
            Slider(value: Binding(get: { s.scrollSpeed },
                                  set: { model.send("scrollSpeed", String(($0 * 4).rounded() / 4)) }), in: 1...4)
            Text(String(format: "%.2g×", s.scrollSpeed)).font(.caption).monospacedDigit().frame(width: 36, alignment: .trailing)
        }
        Toggle("Animate window moves", isOn: model.binding({ $0.animations }, "animations"))
            .toggleStyle(.checkbox)
        HStack {
            Text("Animation speed").font(.caption)
            Slider(value: Binding(get: { s.animationSpeed },
                                  set: { model.send("animationSpeed", String(($0 * 4).rounded() / 4)) }), in: 0.5...2.5)
            Text(String(format: "%.2g×", s.animationSpeed)).font(.caption).monospacedDigit().frame(width: 36, alignment: .trailing)
        }
        .disabled(!s.animations)
        DisclosureGroup("Animation experiments", isExpanded: $showExperiments) {
            ScrollView { experimentList }.frame(maxHeight: 210)
        }
        .font(.caption)
        Picker("Overview", selection: model.choice({ $0.overview }, "overview")) {
            ForEach(Self.overviews, id: \.0) { Text($0.1).tag($0.0) }
        }
        .pickerStyle(.segmented)
        HStack(spacing: 16) {
            Toggle("Focus border", isOn: model.binding({ $0.border }, "border"))
            Toggle("Dim inactive", isOn: model.binding({ $0.dim }, "dim"))
        }
        .toggleStyle(.checkbox)
        HStack {
            Text("Gap").font(.caption)
            Slider(value: Binding(get: { s.gap }, set: { model.send("gap", String($0.rounded())) }), in: 0...32, step: 2)
            Text("\(Int(s.gap)) pt").font(.caption).monospacedDigit().frame(width: 36, alignment: .trailing)
        }
        if !s.swipeAvailable {
            Label("Swipe unavailable (event tap refused)", systemImage: "hand.raised.slash")
                .font(.caption).foregroundStyle(.secondary)
        }
        if !s.failed.isEmpty {
            Label("Shortcuts unavailable: \(s.failed.joined(separator: " "))",
                  systemImage: "keyboard.badge.exclamationmark")
                .font(.caption).foregroundStyle(.orange)
                .fixedSize(horizontal: false, vertical: true)
        }
        DisclosureGroup("Keyboard shortcuts", isExpanded: $showShortcuts) {
            ScrollView { shortcutList }.frame(maxHeight: 170)
        }
        .font(.caption)
    }

    /// Every switch is independent and off by default.
    private var experimentList: some View {
        VStack(alignment: .leading, spacing: 7) {
            ForEach(s.experiments, id: \.key) { e in
                experimentRow(title: e.title, detail: e.detail,
                              isOn: Binding(get: { e.on }, set: { model.send("experiment", "\(e.key)=\($0 ? "1" : "0")") }))
            }
        }
        .padding(.top, 4)
    }

    private func experimentRow(title: String, detail: String, isOn: Binding<Bool>) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Toggle(title, isOn: isOn).toggleStyle(.checkbox)
            Text(detail).font(.caption2).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, 20)
        }
    }

    private var permissionPrompt: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Accessibility access needed", systemImage: "exclamationmark.triangle")
            Text("Reel moves and resizes other apps' windows, which macOS only allows after you approve it in Privacy & Security → Accessibility.")
                .font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Button("Open Accessibility Settings") { model.send("openAccessibility") }
        }
    }

    private var shortcutList: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(s.legend).font(.caption2).foregroundStyle(.secondary)
            ForEach(s.groups, id: \.name) { group in
                Text(group.name).font(.caption.weight(.semibold)).padding(.top, 4)
                ForEach(group.items, id: \.title) { item in
                    VStack(alignment: .leading, spacing: 1) {
                        Text(item.title)
                        Text(item.keys).font(.caption2.monospaced()).foregroundStyle(.secondary)
                    }
                }
            }
            Text("Scroll the strip").font(.caption.weight(.semibold)).padding(.top, 4)
            VStack(alignment: .leading, spacing: 1) {
                Text("Scroll the strip under the pointer")
                Text("\(s.scrollSpelled) + two-finger swipe (or mouse wheel)")
                    .font(.caption2.monospaced()).foregroundStyle(.secondary)
            }
        }
        .padding(.top, 4)
    }

    private var windowList: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("\(s.windows.count) windows").font(.caption).foregroundStyle(.secondary)
            ForEach(Array(s.windows.enumerated()), id: \.offset) { _, w in
                VStack(alignment: .leading, spacing: 1) {
                    Text(w.title.isEmpty ? "(untitled)" : w.title).lineLimit(1)
                    Text("\(w.app) · \(Int(w.w))×\(Int(w.h)) at \(Int(w.x)),\(Int(w.y))\(w.standard ? "" : " · non-standard")\(w.minimized ? " · minimized" : "")")
                        .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        }
    }
}

// MARK: Preview (development)

/// Draws the panel for the given state JSON into a PNG at `path` (no menu bar item).
/// Used with `Reel --render-panel <path>` to check the layout.
@_cdecl("reel_panel_render")
public func reelPanelRender(_ json: UnsafePointer<CChar>, _ path: UnsafePointer<CChar>) -> Bool {
    MainActor.assumeIsolated {
        let text = String(cString: json)
        guard let state = try? JSONDecoder().decode(PanelState.self, from: Data(text.utf8)) else { return false }
        let noop: ReelActionFn = { _, _ in }
        let model = PanelModel(bridge: Bridge(state: { nil }, free: { _ in }, action: noop))
        model.preview(state)
        let view = NSHostingView(rootView: PanelView(model: model).background(Color(nsColor: .windowBackgroundColor)))
        view.frame = NSRect(origin: .zero, size: PanelView.size)
        let window = NSWindow(contentRect: view.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return false }
        view.cacheDisplay(in: view.bounds, to: rep)
        guard let png = rep.representation(using: .png, properties: [:]) else { return false }
        return (try? png.write(to: URL(fileURLWithPath: String(cString: path)))) != nil
    }
}

extension PanelModel {
    func preview(_ state: PanelState) { s = state }
}
