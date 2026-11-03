// Plan PN4: Record my thoughts starts at once, with no consent reminder (it is just you).

import XCTest

final class ThoughtsUITests: XCTestCase {
    @MainActor func testRecordMyThoughtsStartsAtOnceWithNoReminder() {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launchEnvironment["QUIRE_MODEL_DIR"] = NSTemporaryDirectory()
        app.launch()
        addUIInterruptionMonitor(withDescription: "Microphone") { alert in
            let allow = alert.buttons["Allow"]
            guard allow.exists else { return false }
            allow.tap()
            return true
        }
        app.buttons["Record my thoughts"].tap()
        XCTAssertTrue(app.buttons["Stop and close"].waitForExistence(timeout: 5))
        app.tap()
        XCTAssertFalse(app.staticTexts["Tell people you're recording"].waitForExistence(timeout: 2))
        XCTAssertFalse(app.navigationBars["Record"].exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.lifetime = .keepAlways
        add(shot)
        app.buttons["Stop and close"].tap()
    }
}
