import SwiftUI

/// Minimal UI-test host app. Automation runs in `EidolonXcuiHelperUITests`
/// (XCUITest), not here. Do not unarchive kmobile.
@main
struct EidolonXcuiHelperApp: App {
    var body: some Scene {
        WindowGroup {
            Text("Eidolon XCUI Helper")
                .font(.title2)
                .padding()
                .accessibilityIdentifier("eidolon.xcui.host")
        }
    }
}
