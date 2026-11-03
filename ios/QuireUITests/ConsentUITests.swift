// PI24: the desktop's "Read more" guidance on the phone, from Settings and from the reminder.

import XCTest

final class ConsentUITests: XCTestCase {
    @MainActor func testTheGuidanceOpensFromSettingsWithLinesToCopy() {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launch()
        app.buttons["Settings"].tap()
        app.buttons["Telling people you're recording"].tap()
        XCTAssertTrue(app.staticTexts["Why it matters"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["This page is general guidance, not legal advice."].exists || { app.swipeUp(); return app.staticTexts["This page is general guidance, not legal advice."].exists }())
        let copy = app.buttons["Copy the spoken line"]
        XCTAssertTrue(copy.exists)
        copy.tap()
        XCTAssertTrue(app.buttons["Spoken line copied"].waitForExistence(timeout: 2))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "consent-guide"
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor func testTheReminderLeadsToTheGuidance() {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launchEnvironment["QUIRE_MODEL_DIR"] = NSTemporaryDirectory()
        app.launch()
        app.buttons["Record"].tap()
        app.buttons["Start recording"].tap()
        XCTAssertTrue(app.buttons["Read more"].waitForExistence(timeout: 5))
        app.buttons["Read more"].tap()
        XCTAssertTrue(app.staticTexts["What to do"].waitForExistence(timeout: 5))
    }
}
