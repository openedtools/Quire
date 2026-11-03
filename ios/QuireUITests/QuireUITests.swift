// AC-IOS-2: the desktop vault shown read-only, using the shared, fictional fixture vault.

import XCTest

final class QuireUITests: XCTestCase {
    /// A fresh copy of the shared fictional vault: the app writes its own recordings into the vault
    /// it opens, so the repository's fixture is never opened directly
    private var fixtureVault: String {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("docs/vault-fixtures/canonical")
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("vault-\(UUID().uuidString)")
        try? FileManager.default.copyItem(at: source, to: copy)
        return copy.path
    }

    @MainActor private func launch(vault: String?) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["QUIRE_FRESH_DEFAULTS"] = "1"
        if let vault { app.launchEnvironment["QUIRE_VAULT_PATH"] = vault }
        app.launch()
        return app
    }

    @MainActor func testWithoutAVaultTheHomeScreenOffersToChooseOne() {
        let app = launch(vault: nil)
        XCTAssertTrue(app.staticTexts["Record & Transcribe"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Choose the vault folder"].exists)
    }

    @MainActor func testDesktopMeetingsFoldersAndPeopleAppearReadOnly() {
        let app = launch(vault: fixtureVault)
        XCTAssertTrue(app.staticTexts["Garden Club"].waitForExistence(timeout: 5))
        app.buttons.containing(.staticText, identifier: "Garden Club").firstMatch.tap()
        app.staticTexts["Garden club budget"].tap()
        XCTAssertTrue(app.staticTexts["Welcome to the garden club budget meeting."].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Alex Rivera"].exists, "the speaker is linked to a person")
        app.buttons["Summary"].tap()
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Buy one bench'")).firstMatch.exists)
        XCTAssertTrue(app.staticTexts["From the desktop. Read-only on this iPhone for now."].exists)

        XCTAssertTrue(app.staticTexts["From the desktop. Read-only on this iPhone for now."].exists)
        XCTAssertFalse(app.buttons["Delete audio now"].exists, "desktop meetings are read-only (F-IOS-VAULT-4)")

        app.tabBars.buttons["People"].tap()
        XCTAssertTrue(app.staticTexts["Alex Rivera"].waitForExistence(timeout: 5))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor func testRetentionIsChosenInSettingsWithItsLimitsSaidPlainly() {
        let app = launch(vault: nil)
        app.buttons["Settings"].tap()
        let picker = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Keep audio'")).firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertEqual(picker.label, "Keep audio, 7 days", "the desktop's default")
        // The limits are under the retention section's "More about this" (plan PN5): vault first, retention second
        app.buttons.matching(identifier: "More about this").element(boundBy: 1).tap()
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'outside Quire\\'s control'")).firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'cannot delete anything'")).firstMatch.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "settings-retention"
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor func testWithAVaultARecordingCanGoIntoADesktopFolder() {
        let app = launch(vault: fixtureVault)
        XCTAssertTrue(app.staticTexts["Garden Club"].waitForExistence(timeout: 5))
        app.buttons["Record"].tap()
        let picker = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Save in'")).firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertEqual(picker.label, "Save in, Unfiled", "unfiled unless a folder is chosen (PI14)")
        picker.tap()
        app.buttons["Garden Club"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Save in, Garden Club"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH 'The recording moves into the vault when it ends.'")).firstMatch.exists)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "record-into-folder"
        shot.lifetime = .keepAlways
        add(shot)
    }
}
