// AC-IOS-REC-1, 2, 6: starting a recording, the transcript-only choice, the model download that
// asks first, and the consent reminder in the desktop's wording.

import XCTest

final class RecordingUITests: XCTestCase {
    @MainActor private func launch(modelReady: Bool) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        // A folder is enough for the screens; the transcription itself is tested in QuireTests
        if modelReady { app.launchEnvironment["QUIRE_MODEL_DIR"] = NSTemporaryDirectory() }
        app.launch()
        addUIInterruptionMonitor(withDescription: "Microphone") { alert in
            let allow = alert.buttons["Allow"]
            guard allow.exists else { return false }
            allow.tap()
            return true
        }
        return app
    }

    /// Taps the switch itself (a tap on the label does not flip it)
    @MainActor private func flip(_ toggle: XCUIElement) {
        toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.93, dy: 0.5)).tap()
    }

    @MainActor private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor func testWithoutTheModelRecordingWaitsForADownloadThatAsksFirst() {
        let app = launch(modelReady: false)
        app.buttons["Record"].tap()
        XCTAssertTrue(app.navigationBars["Record"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Start recording"].isEnabled, "no recording until the model is here")
        let download = app.buttons.containing(NSPredicate(format: "label BEGINSWITH 'Download ('")).firstMatch
        XCTAssertTrue(download.exists)
        download.tap()
        XCTAssertTrue(app.alerts["Download the transcription model?"].waitForExistence(timeout: 5))
        keep(app, "model-download")
        app.alerts.buttons["Cancel"].tap()
        XCTAssertTrue(download.exists, "cancel downloads nothing")
    }

    @MainActor func testTheReminderShowsTheDesktopWordingAndCanBeTurnedOff() {
        let app = launch(modelReady: true)
        app.buttons["Record"].tap()
        app.buttons["Start recording"].tap()
        XCTAssertTrue(app.staticTexts["Tell people you're recording"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH 'Rules about recording and transcribing conversations differ by place'")).firstMatch.exists)
        app.tap() // lets the microphone prompt be answered
        keep(app, "consent-reminder")
        flip(app.switches["Never show this again"])
        XCTAssertEqual(app.switches["Never show this again"].value as? String, "1")
        app.buttons["Got it"].tap()
        XCTAssertFalse(app.staticTexts["Tell people you're recording"].exists)

        app.buttons["Stop and close"].tap()
        XCTAssertTrue(app.buttons["Record"].waitForExistence(timeout: 10))
        // The second time one tap records (plan PN3)
        app.buttons["Record"].tap()
        XCTAssertTrue(app.staticTexts["New Recording"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Tell people you're recording"].exists, "turned off stays off")
    }

    @MainActor func testTranscriptOnlyIsShownOnTheRecordingScreen() {
        let app = launch(modelReady: true)
        app.buttons["Record"].tap()
        let toggle = app.switches["Transcript only: do not save audio"]
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? String, "0", "audio is kept unless the person turns this on")
        flip(toggle)
        XCTAssertEqual(toggle.value as? String, "1")
        app.buttons["Start recording"].tap()
        XCTAssertTrue(app.staticTexts["On this iPhone · Transcript only"].waitForExistence(timeout: 5))
        keep(app, "recording-transcript-only")
    }
}
