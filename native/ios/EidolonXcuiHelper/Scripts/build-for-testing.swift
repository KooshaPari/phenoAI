#!/usr/bin/env swift
// Build host + UITest for Simulator into ./build (durable; never /tmp).
// Usage: Scripts/build-for-testing.sh [udid]
// Env: EIDOLON_MOBILE_DEVICE / first booted simulator when udid omitted.
// wraps: xcodebuild (system)

import Foundation

let root = URL(fileURLWithPath: CommandLine.arguments[0])
    .resolvingSymlinksInPath()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

let buildDir = root.appendingPathComponent("build", isDirectory: true)
let project = root.appendingPathComponent("EidolonXcuiHelper.xcodeproj")
let stamp = buildDir.appendingPathComponent(".eidolon-xcui-built")
let runnerSrc = root.appendingPathComponent("Scripts/eidolon-xcui-xctest")
let runnerDst = buildDir.appendingPathComponent("eidolon-xcui-xctest")

func fail(_ msg: String) -> Never {
    fputs("error: \(msg)\n", stderr)
    exit(1)
}

#if !os(macOS)
fail("build-for-testing is macOS-only")
#else

try? FileManager.default.createDirectory(at: buildDir, withIntermediateDirectories: true)

func resolveUdid() -> String {
    if CommandLine.arguments.count > 1 {
        return CommandLine.arguments[1]
    }
    if let env = ProcessInfo.processInfo.environment["EIDOLON_MOBILE_DEVICE"], !env.isEmpty {
        return env
    }
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
    proc.arguments = ["simctl", "list", "devices", "booted", "-j"]
    let pipe = Pipe()
    proc.standardOutput = pipe
    try? proc.run()
    proc.waitUntilExit()
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    let text = String(data: data, encoding: .utf8) ?? ""
    // Rough UDID grab: 8-4-4-4-12 hex
    let pattern = try! NSRegularExpression(
        pattern: #"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"#
    )
    let range = NSRange(text.startIndex..., in: text)
    if let match = pattern.firstMatch(in: text, range: range),
       let swiftRange = Range(match.range, in: text)
    {
        return String(text[swiftRange])
    }
    fail(
        "no Simulator UDID — pass UDID, set EIDOLON_MOBILE_DEVICE, or boot a Simulator"
    )
}

let udid = resolveUdid()
fputs("building for udid=\(udid) derivedData=\(buildDir.path)\n", stderr)

let build = Process()
build.executableURL = URL(fileURLWithPath: "/usr/bin/xcodebuild")
build.arguments = [
    "build-for-testing",
    "-project", project.path,
    "-scheme", "EidolonXcuiHelper",
    "-destination", "id=\(udid)",
    "-derivedDataPath", buildDir.path,
    "CODE_SIGNING_ALLOWED=NO",
]
build.currentDirectoryURL = root
try? build.run()
build.waitUntilExit()
if build.terminationStatus != 0 {
    fail("xcodebuild build-for-testing failed (status \(build.terminationStatus))")
}

// Install argv runner next to products (discoverable path).
do {
    if FileManager.default.fileExists(atPath: runnerDst.path) {
        try FileManager.default.removeItem(at: runnerDst)
    }
    try FileManager.default.copyItem(at: runnerSrc, to: runnerDst)
    try FileManager.default.setAttributes(
        [.posixPermissions: 0o755],
        ofItemAtPath: runnerDst.path
    )
} catch {
    fail("failed installing runner to build/: \(error)")
}

FileManager.default.createFile(
    atPath: stamp.path,
    contents: Data("built \(Date())\nudid=\(udid)\n".utf8)
)
print(runnerDst.path)
#endif
