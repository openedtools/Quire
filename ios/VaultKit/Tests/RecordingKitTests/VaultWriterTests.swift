// AC-IOS-8: a phone recording becomes a note and sidecar in the vault that the vault reader
// accepts, written sidecar first, read back, and only then removed from the app's own folder
// (plan PI13, PI14; sync-format.md sections 2, 6, 7, 15).

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private let device = "dev-0a1b2c3d"

private func tempDir(_ name: String) -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(name)-\(UUID().uuidString)")
    try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

/// A vault with the desktop's marker and one folder, "Garden Club"
private func makeVault(minReader: Int64 = 1) throws -> (root: URL, folderID: String) {
    let root = tempDir("vault")
    let now = timestampString(Date())
    let marker = Marker(format: "quire-vault", version: 1, minReader: minReader, vaultID: newULID(at: Date()), created: now, createdBy: "dev-desktop1")
    try Data(renderMarker(marker).utf8).write(to: root.appendingPathComponent("quire-format.json"))
    let folderID = newULID(at: Date())
    let dir = root.appendingPathComponent("Garden Club")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    try Data(renderFolder(Folder(id: folderID, name: "Garden Club", created: now, updated: now, updatedBy: "dev-desktop1")).utf8)
        .write(to: dir.appendingPathComponent("_folder.json"))
    return (root, folderID)
}

/// A finished recording in the app's own folder, with an audio file beside it
private func makeRecording(transcriptOnly: Bool = false) throws -> RecordingSession {
    var session = RecordingSession(folder: tempDir("local"), title: "Bench vote", started: Date(timeIntervalSince1970: 1_790_000_000), deviceID: device, transcriptOnly: transcriptOnly, retention: .d7)
    try session.add(text: "We need two more benches.", start: 0.2, end: 2.0)
    try session.bookmark(at: 1.0)
    try session.finish(at: Date(timeIntervalSince1970: 1_790_000_060), duration: 60)
    if let audio = session.audioURL { try Data("not really audio".utf8).write(to: audio) }
    return session
}

@Test func aFinishedRecordingMovesIntoTheVaultRoot() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    let localNote = try Data(contentsOf: session.noteURL)
    let base = try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil)

    let contents = try VaultReader(root: root).read()
    let meeting = try #require(contents.meetings.first { $0.id == session.id })
    #expect(meeting.folderID == nil, "unfiled at the vault root")
    #expect(meeting.segments?.map(\.text) == ["We need two more benches."], "the sidecar arrived and matches")
    #expect(try Data(contentsOf: root.appendingPathComponent("\(session.id).md")) == localNote, "the same bytes as on the phone")
    #expect(base.id == session.id && base.path == "\(session.id).md" && base.sha256 == sha256Hex(localNote))

    let left = try FileManager.default.contentsOfDirectory(atPath: session.folder.path)
    #expect(!left.contains { $0.hasSuffix(".md") || $0.hasSuffix(".transcript.json") }, "the local note and sidecar are gone")
    #expect(left.contains("\(session.id).\(RecordingAudio.fileExtension)"), "audio never enters the vault (F-IOS-VAULT-5)")
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("_audio").path))
}

@Test func aRecordingCanGoIntoAnExistingDesktopFolder() throws {
    let (root, folderID) = try makeVault()
    let session = try makeRecording()
    let base = try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: folderID)
    #expect(base.path == "Garden Club/\(session.id).md")
    let meeting = try #require(try VaultReader(root: root).read().meetings.first { $0.id == session.id })
    #expect(meeting.folderID == folderID)
    let folderFile = try Data(contentsOf: root.appendingPathComponent("Garden Club/_folder.json"))
    #expect(try parseFolder(folderFile).updatedBy == "dev-desktop1", "the folder file is not changed")
}

@Test func aFolderThatIsGoneOrReservedIsRefusedAndNothingMoves() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    #expect(throws: VaultWriteError.noSuchFolder) { try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: newULID(at: Date())) }
    #expect(FileManager.default.fileExists(atPath: session.noteURL.path), "the local copy stays")
}

@Test func aDifferentFileAlreadyThereIsNeverOverwritten() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    let theirs = Data("someone else's file".utf8)
    try theirs.write(to: root.appendingPathComponent("\(session.id).md"))
    #expect(throws: VaultWriteError.nameTaken) { try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil) }
    #expect(try Data(contentsOf: root.appendingPathComponent("\(session.id).md")) == theirs)
    #expect(FileManager.default.fileExists(atPath: session.noteURL.path))
}

@Test func aMoveCutShortIsFinishedNextTime() throws {
    // The app ended after writing to the vault but before removing the local copy
    let (root, _) = try makeVault()
    let session = try makeRecording()
    let local = tempDir("copy")
    for name in try FileManager.default.contentsOfDirectory(atPath: session.folder.path) {
        try FileManager.default.copyItem(at: session.folder.appendingPathComponent(name), to: local.appendingPathComponent(name))
    }
    _ = try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil)
    let again = try VaultWriter(root: root).moveIn(note: local.appendingPathComponent("\(session.id).md"), folderID: nil)
    #expect(again.path == "\(session.id).md")
    #expect(!FileManager.default.fileExists(atPath: local.appendingPathComponent("\(session.id).md").path))
}

@Test func aNewerVaultIsNeverWritten() throws {
    let (root, _) = try makeVault(minReader: 2)
    let session = try makeRecording()
    #expect(throws: VaultWriteError.newerFormat) { try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil) }
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("\(session.id).md").path))
}

@Test func aFolderThatIsNotAVaultIsNeverWritten() throws {
    let root = tempDir("plain")
    let session = try makeRecording()
    #expect(throws: VaultWriteError.notAVault) { try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil) }
}

@Test func aChangeIsWrittenOnlyOverTheCopyThePhoneLastWrote() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    let writer = VaultWriter(root: root)
    let base = try writer.moveIn(note: session.noteURL, folderID: nil)
    let now = Date(timeIntervalSince1970: 1_790_100_000)

    let next = try writer.update(base) { keepAudio(&$0, now: now, deviceID: device) }
    let note = try parseNote(try Data(contentsOf: root.appendingPathComponent(base.path)))
    #expect(note.audio.retention == "keep" && note.rev == next.rev && next.rev > base.rev)

    // Another device changed the file since: the phone does not write over it
    let url = root.appendingPathComponent(base.path)
    var changed = note
    changed.title = "Bench vote (edited on the desktop)"
    changed.rev += 1
    try Data(renderNote(changed).utf8).write(to: url)
    #expect(throws: VaultWriteError.changedElsewhere) { try writer.update(next) { deleteAudioNow(&$0, now: now, deviceID: device) } }
    #expect(try parseNote(try Data(contentsOf: url)).audio.state == "present")
}

@Test func theRecordOfWhatWasWrittenSurvivesARestart() throws {
    let file = tempDir("bases").appendingPathComponent("vault-bases.json")
    var store = BaseStore(file: file)
    let record = BaseRecord(id: newULID(at: Date()), path: "a.md", sha256: String(repeating: "a", count: 64), rev: 3)
    try store.set(record)
    #expect(BaseStore(file: file).records[record.id] == record)
    try store.remove(record.id)
    #expect(BaseStore(file: file).records.isEmpty)
}

@Test func anOlderCopyThePhoneWroteItselfIsReplaced() throws {
    // A move wrote to the vault but the read-back failed; the person then changed the local copy
    let (root, _) = try makeVault()
    let session = try makeRecording()
    let older = try Data(contentsOf: session.noteURL)
    try older.write(to: root.appendingPathComponent("\(session.id).md"))
    var note = try parseNote(older)
    keepAudio(&note, now: Date(timeIntervalSince1970: 1_790_000_100), deviceID: device)
    try Data(renderNote(note).utf8).write(to: session.noteURL)
    _ = try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil)
    #expect(try parseNote(try Data(contentsOf: root.appendingPathComponent("\(session.id).md"))).audio.retention == "keep")
}

@Test func theBaseIsKeptBeforeTheLocalCopyGoes() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    struct Full: Error {}
    #expect(throws: Full.self) { try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil) { _ in throw Full() } }
    #expect(FileManager.default.fileExists(atPath: session.noteURL.path), "no record kept, so the local copy stays")
}

@Test func aLocalChangeDuringTheMoveStaysAndMovesNextTime() throws {
    let (root, _) = try makeVault()
    let session = try makeRecording()
    #expect(throws: VaultWriteError.changedDuringMove) {
        try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil) { _ in
            var note = try parseNote(try Data(contentsOf: session.noteURL))
            deleteAudioNow(&note, now: Date(timeIntervalSince1970: 1_790_000_100), deviceID: device)
            try Data(renderNote(note).utf8).write(to: session.noteURL)
        }
    }
    _ = try VaultWriter(root: root).moveIn(note: session.noteURL, folderID: nil)
    #expect(try parseNote(try Data(contentsOf: root.appendingPathComponent("\(session.id).md"))).audio.state == "deleted")
}

@Test func aLostBaseIsRecoveredOnlyForTheNoteThePhoneWroteLast() throws {
    let (root, folderID) = try makeVault()
    let session = try makeRecording()
    let writer = VaultWriter(root: root)
    let base = try writer.moveIn(note: session.noteURL, folderID: folderID)
    #expect(writer.recoverBase(session.id, deviceID: device) == base)
    #expect(writer.recoverBase(session.id, deviceID: "dev-ffffffff") == nil)
}
