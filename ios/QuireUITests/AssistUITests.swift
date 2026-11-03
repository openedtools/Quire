// AC-IOS-3 and F-IOS-AI-1, 2 on screen: a meeting this iPhone recorded, in a test vault, gets a
// summary and answers with times to tap, made by Apple's model on the device. Where the model is
// not available the app says why instead (PI18). All content is made up.

import RecordingKit
import XCTest

final class AssistUITests: XCTestCase {
    private let device = "dev-7e570001"

    /// A copy of the shared fictional vault with one meeting recorded "on this iPhone"
    private func vaultWithPhoneMeeting() throws -> String {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("docs/vault-fixtures/canonical")
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("vault-\(UUID().uuidString)")
        try FileManager.default.copyItem(at: source, to: copy)
        var session = RecordingSession(folder: copy, title: "Pond benches", started: Date().addingTimeInterval(-600), deviceID: device, transcriptOnly: true)
        let lines = [
            "Welcome everyone, this is the pond committee.",
            "The water tap by the gate costs forty pounds to fix.",
            "We agreed to buy two benches for the pond.",
            "Sam will order the benches by Friday.",
        ]
        for (i, line) in lines.enumerated() { try session.add(text: line, start: Double(i * 20), end: Double(i * 20 + 15)) }
        try session.finish(at: Date().addingTimeInterval(-500), duration: 80)
        return copy.path
    }

    @MainActor private func launch() throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launchEnvironment["QUIRE_VAULT_PATH"] = try vaultWithPhoneMeeting()
        app.launchEnvironment["QUIRE_DEVICE_ID"] = device
        app.launch()
        return app
    }

    @MainActor private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor func testAQuestionIsAnsweredWithTimesToTapAndASummaryIsMade() throws {
        let app = try launch()
        XCTAssertTrue(app.staticTexts["Unfiled"].waitForExistence(timeout: 10))
        app.buttons.containing(.staticText, identifier: "Unfiled").firstMatch.tap()
        app.staticTexts["Pond benches"].tap()
        XCTAssertTrue(app.staticTexts["Recorded on this iPhone."].waitForExistence(timeout: 5))

        // Copy in one tap, on every tab
        app.buttons["Copy the transcript"].tap()
        XCTAssertTrue(app.buttons["Transcript copied"].waitForExistence(timeout: 2))

        app.buttons["Ask about this meeting"].firstMatch.tap()
        let unavailable = app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Apple Intelligence'")).firstMatch
        if unavailable.waitForExistence(timeout: 2) {
            keep(app, "ask-unavailable")
            throw XCTSkip("Apple Intelligence is not on in this simulator; the app says why instead")
        }
        let field = app.textFields["Question about this meeting"]
        field.tap()
        field.typeText("How much does the water tap cost?")
        app.buttons["Ask"].tap()
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH 'AI generated'")).firstMatch.waitForExistence(timeout: 60))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] 'forty' OR label CONTAINS '40'")).firstMatch.exists)
        // The model is asked to cite, not forced to (as on the desktop): when it does, the time jumps to its line
        let citation = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Go to '")).firstMatch
        if citation.exists {
            citation.tap()
            XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'water tap by the gate'")).firstMatch.waitForExistence(timeout: 5))
        }
        keep(app, "ask-answer")

        if app.buttons["Done"].exists { app.buttons["Done"].tap() }
        app.buttons["Summary"].tap()
        app.buttons["Summarize"].tap()
        XCTAssertTrue(app.staticTexts["AI generated on this iPhone."].waitForExistence(timeout: 120))
        XCTAssertTrue(app.buttons["Summarize again"].exists)
        app.buttons["Copy the summary"].tap()
        XCTAssertTrue(app.buttons["Summary copied"].waitForExistence(timeout: 2))
        XCTAssertFalse(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH '# '")).firstMatch.exists, "headings are drawn, not shown as #")
        app.swipeUp()
        keep(app, "summary")
    }

    @MainActor func testDesktopMeetingsCanBeAskedButNotSummarizedAgain() throws {
        let app = try launch()
        XCTAssertTrue(app.staticTexts["Garden Club"].waitForExistence(timeout: 10))
        app.buttons.containing(.staticText, identifier: "Garden Club").firstMatch.tap()
        app.staticTexts["Garden club budget"].tap()
        XCTAssertTrue(app.buttons["Ask about this meeting"].waitForExistence(timeout: 5), "Ask only reads (PI21)")
        app.buttons["Summary"].tap()
        XCTAssertFalse(app.buttons["Summarize again"].exists)
        XCTAssertFalse(app.buttons["Summarize"].exists, "desktop meetings keep their own summaries (PI20)")
    }
}
