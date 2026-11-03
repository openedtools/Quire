// The reader over the shared canonical vault and over hostile changes to it (F-IOS-VAULT-2,
// sync-format.md section 14).

import Foundation
import Testing
@testable import VaultKit

private let canonical = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .appendingPathComponent("docs/vault-fixtures/canonical")

/// A writable copy of the canonical vault.
private func copy() throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("vault-\(UUID().uuidString)")
    try FileManager.default.copyItem(at: canonical, to: dir)
    return dir
}

@Test func theCanonicalVaultReadsCompletely() throws {
    let v = try VaultReader(root: canonical).read()
    #expect(v.marker?.format == "quire-vault")
    #expect(Set(v.folders.map(\.name)) == ["Garden Club", "garden club"])
    #expect(v.people.map(\.name) == ["Alex Rivera"])
    #expect(v.meetings.count == 2)
    let filed = try #require(v.meetings.first { $0.note.title == "Garden club budget" })
    #expect(filed.folderID == "01JC5K3N7P9R2T4V6X8Z0BDFGH")
    #expect(filed.segments?.map(\.text).first == "Welcome to the garden club budget meeting.")
    #expect(v.person(filed.note.speakers.first?.person)?.name == "Alex Rivera")
    let sections = bodySections(filed.note.body)
    #expect(sections.summary == "- Buy one bench.")
    #expect(sections.notes == "Ask about the water tap.")
    #expect(v.skipped.isEmpty)
}

@Test func aFolderThatIsNotAVaultIsRefused() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("empty-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    #expect(throws: VaultReadError.notAVault) { try VaultReader(root: dir).read() }
    try Data("{\"format\":\"other\"}".utf8).write(to: dir.appendingPathComponent("quire-format.json"))
    #expect(throws: VaultReadError.notAVault) { try VaultReader(root: dir).read() }
}

@Test func hostileFilesAreSkippedNeverTrusted() throws {
    let dir = try copy()
    let fm = FileManager.default
    let noteURL = dir.appendingPathComponent("Garden Club/01JC90A2T8N4Y6B3D7F1GHKQ5S.md")
    let original = try String(contentsOf: noteURL, encoding: .utf8)
    // A note whose ID does not match its name
    try original.replacingOccurrences(of: "id: \"01JC90A2T8N4Y6B3D7F1GHKQ5S\"", with: "id: \"01JC90A2T8N4Y6B3D7F1GHKQ5T\"")
        .write(to: dir.appendingPathComponent("01JC90A2T8N4Y6B3D7F1GHKQ5Z.md"), atomically: true, encoding: .utf8)
    // A sidecar path taken from content would escape the vault: it is never used
    try original.replacingOccurrences(of: "file: \"01JC90A2T8N4Y6B3D7F1GHKQ5S.", with: "file: \"../../etc/")
        .write(to: noteURL, atomically: true, encoding: .utf8)
    // A link in place of a folder and an oversized note
    try fm.createSymbolicLink(at: dir.appendingPathComponent("Linked"), withDestinationURL: fm.temporaryDirectory)
    try Data(count: maxNoteBytes + 1).write(to: dir.appendingPathComponent("01JC9C00000000000000000000.md"))

    let v = try VaultReader(root: dir).read()
    #expect(v.skipped["note"] == 2, "the mismatched ID and the oversized file")
    let filed = try #require(v.meetings.first { $0.id == "01JC90A2T8N4Y6B3D7F1GHKQ5S" })
    #expect(filed.segments == nil, "a transcript that does not check out shows as still syncing")
    #expect(v.folders.count == 2, "the link is not followed")
}

@Test func anICloudPlaceholderWaitsInsteadOfDisappearing() throws {
    let dir = try copy()
    let fm = FileManager.default
    let note = dir.appendingPathComponent("01JC9B00000000000000000000.md")
    try fm.moveItem(at: note, to: dir.appendingPathComponent(".01JC9B00000000000000000000.md.icloud"))
    let v = try VaultReader(root: dir).read()
    #expect(v.waitingForICloud == 1)
    #expect(v.meetings.count == 1)
    #expect(v.skipped.isEmpty)
}

// ===== Review findings (I1) =====

@Test func hostileNumbersNeverReachTheApp() throws {
    let dir = try copy()
    let noteURL = dir.appendingPathComponent("Garden Club/01JC90A2T8N4Y6B3D7F1GHKQ5S.md")
    let sidecar = "{\n  \"quire\": 1,\n  \"note_id\": \"01JC90A2T8N4Y6B3D7F1GHKQ5S\",\n  \"segments\": [\n    {\"id\":\"s\",\"start\":1e308,\"end\":1e308,\"speaker\":1,\"text\":\"x\"}\n  ]\n}\n"
    let sha = sha256Hex(Data(sidecar.utf8))
    try Data(sidecar.utf8).write(to: dir.appendingPathComponent("Garden Club/\(sidecarName(noteID: "01JC90A2T8N4Y6B3D7F1GHKQ5S", sha256: sha))"))
    var text = try String(contentsOf: noteURL, encoding: .utf8)
    let old = try parseNote(Data(text.utf8)).transcript!
    text = text.replacingOccurrences(of: old.sha256, with: sha).replacingOccurrences(of: old.file, with: sidecarName(noteID: "01JC90A2T8N4Y6B3D7F1GHKQ5S", sha256: sha))
        .replacingOccurrences(of: "duration_s: 9", with: "duration_s: 9223372036854775807")
    try text.write(to: noteURL, atomically: true, encoding: .utf8)
    let v = try VaultReader(root: dir).read()
    let m = try #require(v.meetings.first { $0.id == "01JC90A2T8N4Y6B3D7F1GHKQ5S" })
    #expect(m.note.durationS == nil)
    #expect(m.segments == nil)
    #expect(v.skipped["transcript"] == 1)
}

@Test func aHashThatIsNotHexNeverBecomesAPath() throws {
    let dir = try copy()
    let noteURL = dir.appendingPathComponent("Garden Club/01JC90A2T8N4Y6B3D7F1GHKQ5S.md")
    let text = try String(contentsOf: noteURL, encoding: .utf8)
    let t = try parseNote(Data(text.utf8)).transcript!
    try text.replacingOccurrences(of: "sha256: \"\(t.sha256)\"", with: "sha256: \"/../../x\"").write(to: noteURL, atomically: true, encoding: .utf8)
    let v = try VaultReader(root: dir).read()
    #expect(v.meetings.first { $0.id == "01JC90A2T8N4Y6B3D7F1GHKQ5S" }?.segments == nil)
    #expect(!isSHA256("/../../x"))
}

@Test func theGrammarWorksOnScalarsLikeTheDesktop() {
    func ok(_ text: String) -> Bool { (try? parseDocument(Data(text.utf8))) != nil }
    #expect(ok("---\ntitle: \"\u{0301}x\"\n---\n"), "a string may start with a combining mark")
    #expect(!ok("---\n  \u{0301}key: 1\n---\n"), "an indented line with a stray mark is not a key")
    #expect(!ok("---\n\u{feff}quire: 1\n---\n"), "a BOM inside the front matter is not a key")
    let (_, body) = try! parseDocument(Data("---\nquire: 1\n---\n\u{feff}body".utf8))
    #expect(body.unicodeScalars.first == "\u{feff}", "the body keeps its bytes")
    var deep = "---\na:\n"
    for level in 1...12 { deep += String(repeating: "  ", count: level) + "b:\n" }
    deep += String(repeating: "  ", count: 13) + "c: 1\n---\n"
    #expect(!ok(deep), "nesting is capped")
}

@Test func jsonIsReadStrictly() {
    let folder = "{\"quire\":1,\"id\":\"01JC5K3N7P9R2T4V6X8Z0BDFGH\",\"name\":\"A\",\"created\":\"x\",\"updated\":\"x\",\"updated_by\":\"d\"}"
    #expect((try? parseFolder(Data(folder.utf8))) != nil)
    #expect((try? parseFolder(Data(folder.replacingOccurrences(of: "\"quire\":1", with: "\"quire\":true").utf8))) == nil)
    #expect((try? parseFolder(Data(folder.replacingOccurrences(of: "\"quire\":1", with: "\"quire\":1.9").utf8))) == nil)
    #expect((try? parseFolder(Data(folder.replacingOccurrences(of: "\"name\":\"A\"", with: "\"name\":\"A\",\"name\":\"B\"").utf8))) == nil)
    #expect((try? parseFolder(folder.data(using: .utf16)!)) == nil)
    #expect(!hasDuplicateKeys(Data("{\"a\":[{\"b\":1},{\"b\":2}],\"c\":{\"b\":1}}".utf8)), "the same key in different objects is fine")
}

@Test func bodiesWithoutHeadingsOrWithCRLFStillShow() {
    #expect(bodySections("\nPlant tulips.\n").notes == "Plant tulips.")
    let crlf = bodySections("\r\n## Summary\r\n\r\n- A\r\n\r\n## Notes\r\n\r\nB\r\n")
    #expect(crlf.summary == "- A")
    #expect(crlf.notes == "B")
}

@Test func aNewerVaultOpensReadOnly() throws {
    let dir = try copy()
    let marker = Marker(format: "quire-vault", version: 2, minReader: 2, vaultID: "01JC8Z00000000000000000000", created: "x", createdBy: "dev-1")
    try Data(renderMarker(marker).utf8).write(to: dir.appendingPathComponent("quire-format.json"))
    let v = try VaultReader(root: dir).read()
    #expect(v.newerFormat)
    #expect(v.meetings.count == 2)
}

@Test func aHalfWrittenMarkerMeansTryAgainLater() throws {
    let dir = try copy()
    try Data("{\"format\": \"quire-vault\", \"vers".utf8).write(to: dir.appendingPathComponent("quire-format.json"))
    #expect(throws: VaultReadError.notReady) { try VaultReader(root: dir).read() }
}
