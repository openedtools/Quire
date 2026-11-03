// Plan PN1 and PN2: the Meetings tab (folders first, then meetings in no folder, with search),
// over the shared, fictional fixture vault.

import XCTest

final class MeetingsUITests: XCTestCase {
    private var fixtureVault: String {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("docs/vault-fixtures/canonical")
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("vault-\(UUID().uuidString)")
        try? FileManager.default.copyItem(at: source, to: copy)
        return copy.path
    }

    @MainActor private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        app.launchEnvironment["QUIRE_VAULT_PATH"] = fixtureVault
        app.launch()
        return app
    }

    @MainActor func testMeetingsShowsFoldersFirstAndSearches() {
        let app = launch()
        app.tabBars.buttons["Meetings"].tap()
        XCTAssertTrue(app.navigationBars["Meetings"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Folders"].exists)
        XCTAssertTrue(app.buttons.containing(.staticText, identifier: "Garden Club").firstMatch.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.lifetime = .keepAlways
        add(shot)

        let search = app.searchFields["Search meetings"]
        if !search.exists { app.swipeDown() }
        search.tap()
        search.typeText("budget")
        XCTAssertTrue(app.staticTexts["Garden club budget"].waitForExistence(timeout: 5))
        search.typeText(" nothing like this")
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'No Results'")).firstMatch.waitForExistence(timeout: 5))
    }

    @MainActor func testAFolderCanBeMadeOnThePhone() {
        let app = launch()
        app.tabBars.buttons["Meetings"].tap()
        app.buttons["New folder"].tap()
        let field = app.textFields["Name"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("Choir")
        app.buttons["Make folder"].tap()
        XCTAssertTrue(app.buttons.containing(.staticText, identifier: "Choir").firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'two-way sync'")).firstMatch.exists)
    }
}
