// AC-IOS-10, F-IOS-UI-1: every control on every screen has a spoken label (none falls back to a
// symbol name), and the screens are captured in dark mode and at the largest accessibility text
// size for review. All content is made up.

import RecordingKit
import XCTest

final class AccessibilityUITests: XCTestCase {
    private let device = "dev-7e570002"

    private func vaultWithPhoneMeeting() throws -> String {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("docs/vault-fixtures/canonical")
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("vault-\(UUID().uuidString)")
        try FileManager.default.copyItem(at: source, to: copy)
        var session = RecordingSession(folder: copy, title: "Pond benches", started: Date().addingTimeInterval(-600), deviceID: device, transcriptOnly: true)
        try session.add(text: "We agreed to buy two benches for the pond.", start: 2, end: 6)
        try session.bookmark(at: 3)
        try session.finish(at: Date().addingTimeInterval(-500), duration: 10)
        return copy.path
    }

    @MainActor private func launch(largeText: Bool = false) throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launchEnvironment["QUIRE_VAULT_PATH"] = try vaultWithPhoneMeeting()
        app.launchEnvironment["QUIRE_DEVICE_ID"] = device
        app.launchEnvironment["QUIRE_MODEL_DIR"] = NSTemporaryDirectory()
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        return app
    }

    /// Every button, switch, slider and text field has a label a person would say, on the whole
    /// screen: checked, then scrolled and checked again until the bottom
    @MainActor private func checkLabels(_ app: XCUIApplication, _ screen: String) {
        for _ in 0..<6 {
            checkVisibleLabels(app, screen)
            let before = app.debugDescription.count
            app.swipeUp()
            if app.debugDescription.count == before { break }
        }
        for _ in 0..<6 { app.swipeDown() }
    }

    @MainActor private func checkVisibleLabels(_ app: XCUIApplication, _ screen: String) {
        // Let sheets and the reminder finish sliding in, so every frame is read at rest
        Thread.sleep(forTimeInterval: 1)
        let switches = app.switches.allElementsBoundByIndex
        let labelledSwitches = switches.filter { !$0.label.isEmpty }.map(\.frame)
        let controls = app.buttons.allElementsBoundByIndex + switches
            + app.sliders.allElementsBoundByIndex + app.textFields.allElementsBoundByIndex
        for control in controls where control.exists && control.isHittable {
            // A toggle's inner switch sits inside the labelled toggle; VoiceOver reads them as one
            if control.elementType == .switch, control.label.isEmpty, labelledSwitches.contains(where: { $0.insetBy(dx: -4, dy: -1).contains(control.frame) }) { continue }
            let label = control.label.trimmingCharacters(in: .whitespaces)
            XCTAssertFalse(label.isEmpty, "\(screen): a \(control.elementType) at \(control.frame) has no label; switches: \(switches.map { "\($0.label)@\($0.frame)" })")
            XCTAssertNil(label.wholeMatch(of: /[a-z]+(\.[a-z]+)+/), "\(screen): \"\(label)\" is a symbol name, not a label")
        }
    }

    @MainActor private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor private func walk(_ app: XCUIApplication, prefix: String) {
        XCTAssertTrue(app.buttons["Record"].waitForExistence(timeout: 10))
        checkLabels(app, "Home")
        keep(app, "\(prefix)-home")

        app.buttons.containing(.staticText, identifier: "Unfiled").firstMatch.tap()
        app.staticTexts["Pond benches"].tap()
        XCTAssertTrue(app.buttons["Ask about this meeting"].waitForExistence(timeout: 5))
        checkLabels(app, "Meeting")
        keep(app, "\(prefix)-meeting")
        app.buttons["Ask about this meeting"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Done"].waitForExistence(timeout: 5))
        checkLabels(app, "Ask")
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        app.navigationBars.buttons.firstMatch.tap()

        app.buttons["Settings"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
        checkLabels(app, "Settings")
        keep(app, "\(prefix)-settings")
        app.buttons["Done"].tap()

        app.buttons["Record"].tap()
        XCTAssertTrue(app.navigationBars["Record"].waitForExistence(timeout: 5))
        checkLabels(app, "Record sheet")
        // At the largest text sizes the sheet scrolls
        for _ in 0..<4 where !app.buttons["Start recording"].isHittable { app.swipeUp() }
        app.buttons["Start recording"].tap()
        XCTAssertTrue(app.staticTexts["New Recording"].waitForExistence(timeout: 5))
        checkLabels(app, "Recording")
        keep(app, "\(prefix)-recording")
        for _ in 0..<4 where !app.buttons["Read more"].isHittable { app.swipeUp() }
        app.buttons["Read more"].tap()
        XCTAssertTrue(app.staticTexts["Why it matters"].waitForExistence(timeout: 5))
        checkLabels(app, "Consent guidance")
        keep(app, "\(prefix)-consent")
    }

    @MainActor func testEveryControlIsLabelledInDarkMode() throws {
        XCUIDevice.shared.appearance = .dark
        defer { XCUIDevice.shared.appearance = .light }
        walk(try launch(), prefix: "dark")
    }

    @MainActor func testEveryControlIsLabelledAtTheLargestTextSize() throws {
        walk(try launch(largeText: true), prefix: "large")
    }
}
