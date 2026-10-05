import AppIntents
import AppKit

// The extension runs only for an explicit system action. The Rust taskbar does
// not load a Swift runtime, index window titles, or run a background helper.
enum RowlaAction: String {
    case sort, show, hide
}

enum RowlaActionBridge {
    @MainActor
    static func send(_ action: RowlaAction, application: URL? = nil) async throws {
        let host = application ?? Bundle.main.bundleURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        guard host.pathExtension == "app",
              let url = URL(string: "rowla://\(action.rawValue)") else {
            throw BridgeError.invalidApplication
        }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = false
        _ = try await NSWorkspace.shared.open(
            [url], withApplicationAt: host, configuration: configuration)
    }

    enum BridgeError: LocalizedError {
        case invalidApplication
        var errorDescription: String? { "Rowla's application bundle is unavailable." }
    }
}

struct SortWindowsIntent: AppIntent {
    static var title: LocalizedStringResource = "Sort Windows"
    static var description = IntentDescription("Restore your application order in Rowla.")
    static var openAppWhenRun = false
    func perform() async throws -> some IntentResult {
        try await RowlaActionBridge.send(.sort)
        return .result()
    }
}

struct ShowTaskbarsIntent: AppIntent {
    static var title: LocalizedStringResource = "Show Taskbars"
    static var description = IntentDescription("Show all Rowla taskbars.")
    static var openAppWhenRun = false
    func perform() async throws -> some IntentResult {
        try await RowlaActionBridge.send(.show)
        return .result()
    }
}

struct HideTaskbarsIntent: AppIntent {
    static var title: LocalizedStringResource = "Hide Taskbars"
    static var description = IntentDescription("Hide all Rowla taskbars. Use Show Taskbars to restore them.")
    static var openAppWhenRun = false
    func perform() async throws -> some IntentResult {
        try await RowlaActionBridge.send(.hide)
        return .result()
    }
}
