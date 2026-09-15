import XCTest

/// XCUITest action runner for the `BundleXcuiBridge` argv contract.
///
/// The macOS runner (`Scripts/eidolon-xcui-xctest`) injects
/// `EIDOLON_XCUI_REQUEST_JSON` into the test process (via `.xctestrun`
/// EnvironmentVariables) and parses marked lines from `xcodebuild` logs.
/// Simulator UITests cannot read the host `build/` directory.
///
/// Supported ops (device points via XCUICoordinate):
/// - `tap` / `swipe` / `viewport`
/// - `text` — requires an active first responder; fails loud otherwise
///
/// Unsupported ops fail loud (never invent success).
final class EidolonXcuiActions: XCTestCase {
    private static let hostBundleId = "org.phenotype.eidolon.EidolonXcuiHelper"
    private static let markerPrefix = "EIDOLON_XCUI_RESULT:"

    func testRunRequestedOp() throws {
        let request = try Self.loadRequestFromEnv()
        do {
            let result = try Self.execute(request)
            Self.emit(result)
            if !result.ok {
                XCTFail(result.error ?? "eidolon xcui op failed")
            }
        } catch {
            let failure = OpResult(ok: false, stdout: "", error: String(describing: error))
            Self.emit(failure)
            throw error
        }
    }

    // MARK: - Execute

    private static func execute(_ request: Request) throws -> OpResult {
        switch request.op {
        case "tap":
            let x = try requireInt(request.x, name: "x")
            let y = try requireInt(request.y, name: "y")
            try tap(x: x, y: y)
            return OpResult(ok: true, stdout: "", error: nil)
        case "swipe":
            let x1 = try requireInt(request.x1, name: "x1")
            let y1 = try requireInt(request.y1, name: "y1")
            let x2 = try requireInt(request.x2, name: "x2")
            let y2 = try requireInt(request.y2, name: "y2")
            try swipe(x1: x1, y1: y1, x2: x2, y2: y2)
            return OpResult(ok: true, stdout: "", error: nil)
        case "text":
            let value = try requireString(request.value, name: "value")
            guard !value.isEmpty else {
                throw OpError("text --value must be non-empty")
            }
            try inputText(value)
            return OpResult(ok: true, stdout: "", error: nil)
        case "viewport":
            return OpResult(ok: true, stdout: viewportLine(), error: nil)
        default:
            throw OpError(
                "unsupported op `\(request.op)` — supported: tap|swipe|text|viewport; "
                    + "failing loud (never invent success)"
            )
        }
    }

    private static func tap(x: Int, y: Int) throws {
        let app = frontmostApp()
        let coord = app.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: CGFloat(x), dy: CGFloat(y)))
        coord.tap()
    }

    private static func swipe(x1: Int, y1: Int, x2: Int, y2: Int) throws {
        let app = frontmostApp()
        let start = app.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: CGFloat(x1), dy: CGFloat(y1)))
        let end = app.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: CGFloat(x2), dy: CGFloat(y2)))
        start.press(forDuration: 0.05, thenDragTo: end)
    }

    private static func inputText(_ value: String) throws {
        // Honesty: XCUI `typeText` needs keyboard focus / first responder.
        let app = frontmostApp()
        guard app.wait(for: .runningForeground, timeout: 3) || app.exists else {
            throw OpError(
                "text requires a running target app with keyboard focus — "
                    + "launch/focus a text field then retry (XCUI typeText limitation)"
            )
        }
        app.typeText(value)
    }

    private static func viewportLine() -> String {
        let bounds = XCUIScreen.main.bounds
        let scale = XCUIScreen.main.scale
        let w = Int(bounds.width.rounded())
        let h = Int(bounds.height.rounded())
        return "\(w) \(h) \(scale)"
    }

    /// Prefer the Eidolon host when installed; otherwise SpringBoard (screen points).
    private static func frontmostApp() -> XCUIApplication {
        let host = XCUIApplication(bundleIdentifier: hostBundleId)
        if host.state == .runningForeground || host.state == .runningBackground {
            return host
        }
        if host.state == .notRunning {
            host.launch()
            if host.wait(for: .runningForeground, timeout: 5) {
                return host
            }
        }
        return XCUIApplication(bundleIdentifier: "com.apple.springboard")
    }

    // MARK: - IO

    private static func loadRequestFromEnv() throws -> Request {
        let env = ProcessInfo.processInfo.environment
        guard let raw = env["EIDOLON_XCUI_REQUEST_JSON"], !raw.isEmpty else {
            throw OpError(
                "missing EIDOLON_XCUI_REQUEST_JSON — invoke via Scripts/eidolon-xcui-xctest"
            )
        }
        guard let data = raw.data(using: .utf8) else {
            throw OpError("EIDOLON_XCUI_REQUEST_JSON is not UTF-8")
        }
        return try JSONDecoder().decode(Request.self, from: data)
    }

    /// Emit a single-line JSON payload prefixed for the host runner to scrape.
    private static func emit(_ result: OpResult) {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(result),
              let line = String(data: data, encoding: .utf8)
        else {
            print("\(markerPrefix){\"ok\":false,\"stdout\":\"\",\"error\":\"encode failed\"}")
            return
        }
        print("\(markerPrefix)\(line)")
        fflush(stdout)
    }

    private static func requireInt(_ value: Int?, name: String) throws -> Int {
        guard let value else {
            throw OpError("missing required field `\(name)`")
        }
        return value
    }

    private static func requireString(_ value: String?, name: String) throws -> String {
        guard let value else {
            throw OpError("missing required field `\(name)`")
        }
        return value
    }
}

// MARK: - Models

private struct Request: Decodable {
    let op: String
    let udid: String
    let x: Int?
    let y: Int?
    let x1: Int?
    let y1: Int?
    let x2: Int?
    let y2: Int?
    let value: String?
}

private struct OpResult: Codable {
    let ok: Bool
    let stdout: String
    let error: String?
}

private struct OpError: Error, CustomStringConvertible {
    let message: String
    init(_ message: String) { self.message = message }
    var description: String { message }
}
