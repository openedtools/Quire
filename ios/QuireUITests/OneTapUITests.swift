// Plan PN3: one tap records with the last choices once they have been seen; the options open
// the first time, on a long press, and from "Recording options".

import XCTest

final class OneTapUITests: XCTestCase {
    @MainActor private func launch() -> XCUIApplication {
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
        return app
    }

    @MainActor func testTheSecondTapRecordsAtOnce() {
        let app = launch()
        app.buttons["Record"].tap()
        XCTAssertTrue(app.navigationBars["Record"].waitForExistence(timeout: 5), "the first time shows the choices")
        app.buttons["Start recording"].tap()
        let gotIt = app.buttons["Got it"]
        if gotIt.waitForExistence(timeout: 5) { gotIt.tap() }
        app.tap()
        app.buttons["Stop and close"].tap()

        XCTAssertTrue(app.buttons["Record"].waitForExistence(timeout: 10))
        app.buttons["Record"].tap()
        XCTAssertTrue(app.buttons["Stop and close"].waitForExistence(timeout: 5), "straight to recording")
        XCTAssertFalse(app.navigationBars["Record"].exists)
        app.buttons["Stop and close"].tap()

        XCTAssertTrue(app.buttons["Recording options"].waitForExistence(timeout: 10))
        app.buttons["Recording options"].tap()
        XCTAssertTrue(app.navigationBars["Record"].waitForExistence(timeout: 5))
    }
}
