// Plan PN2: folders made on the phone, in the vault's folder format (sync-format.md section 9),
// and the phone's own meetings moved between them. Every name here is made up.

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private let phone = "dev-0a1b2c3d"
private let now = Date(timeIntervalSince1970: 1_790_000_000)

private func vault() throws -> (root: URL, desktopFolder: String) {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("folders-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let stamp = timestampString(now)
    let marker = Marker(format: "quire-vault", version: 1, minReader: 1, vaultID: newULID(at: now), created: stamp, createdBy: "dev-desktop1")
    try Data(renderMarker(marker).utf8).write(to: root.appendingPathComponent("quire-format.json"))
    let id = newULID(at: now.addingTimeInterval(-100))
    let dir = root.appendingPathComponent("Garden Club")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    try Data(renderFolder(Folder(id: id, name: "Garden Club", created: stamp, updated: stamp, updatedBy: "dev-desktop1")).utf8).write(to: dir.appendingPathComponent("_folder.json"))
    return (root, id)
}

private func folders(_ root: URL) throws -> [Folder] {
    try VaultReader(root: root).read().folders
}

@Test func aFolderMadeOnThePhoneIsAValidVaultFolder() throws {
    let (root, _) = try vault()
    let made = try VaultWriter(root: root).createFolder(named: "  Choir  ", deviceID: phone, now: now)
    #expect(made.name == "Choir" && made.updatedBy == phone && isULID(made.id))
    let read = try folders(root)
    #expect(read.contains { $0.id == made.id && $0.name == "Choir" })
    #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("Choir/_folder.json").path))
    #expect(throws: VaultWriteError.emptyName) { try VaultWriter(root: root).createFolder(named: "   ", deviceID: phone, now: now) }
}

@Test func aNameAlreadyUsedGetsANumber() throws {
    let (root, _) = try vault()
    let made = try VaultWriter(root: root).createFolder(named: "garden club", deviceID: phone, now: now)
    #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("garden club (2)/_folder.json").path))
    #expect(try folders(root).count == 2 && made.name == "garden club")
}

@Test func onlyThePhonesOwnFoldersAreRenamed() throws {
    let (root, desktop) = try vault()
    let writer = VaultWriter(root: root)
    let made = try writer.createFolder(named: "Choir", deviceID: phone, now: now)
    let (renamed, from, to) = try writer.renameFolder(made.id, to: "Choir practice", deviceID: phone, now: now.addingTimeInterval(60))
    #expect(from == "Choir" && to == "Choir practice")
    #expect(renamed.name == "Choir practice" && renamed.created == made.created && renamed.updated != made.updated)
    #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("Choir practice/_folder.json").path))
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("Choir").path))
    #expect(throws: VaultWriteError.notThisPhonesFolder) { try writer.renameFolder(desktop, to: "Mine now", deviceID: phone, now: now) }
}

@Test func aMeetingMovesWithItsTranscriptAndOnlyIfUnchanged() throws {
    let (root, desktop) = try vault()
    let local = FileManager.default.temporaryDirectory.appendingPathComponent("local-\(UUID().uuidString)")
    var session = RecordingSession(folder: local, title: "Bench vote", started: now, deviceID: phone, transcriptOnly: true)
    try session.add(text: "We need two more benches.", start: 0.2, end: 2.0)
    try session.finish(at: now.addingTimeInterval(60), duration: 60)
    let writer = VaultWriter(root: root)
    let base = try writer.moveIn(note: session.noteURL, folderID: nil)

    let moved = try writer.moveNote(base, toFolder: desktop)
    #expect(moved.path == "Garden Club/\(session.id).md")
    let meeting = try #require(try VaultReader(root: root).read().meetings.first { $0.id == session.id })
    #expect(meeting.folderID == desktop && meeting.segments?.count == 1, "the transcript came along")
    #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("\(session.id).md").path))

    let back = try writer.moveNote(moved, toFolder: nil)
    #expect(back.path == "\(session.id).md")
    #expect(try writer.moveNote(back, toFolder: nil) == back, "already there")

    // Changed by another device since: left alone
    let url = root.appendingPathComponent(back.path)
    try (String(decoding: try Data(contentsOf: url), as: UTF8.self) + "\nAdded elsewhere.\n").write(to: url, atomically: true, encoding: .utf8)
    #expect(throws: VaultWriteError.changedElsewhere) { try writer.moveNote(back, toFolder: desktop) }
}

@Test func aRenameInLetterCaseOnlyWorksAndBasesFollowARename() throws {
    let (root, _) = try vault()
    let writer = VaultWriter(root: root)
    let made = try writer.createFolder(named: "choir", deviceID: phone, now: now)
    let (_, from, to) = try writer.renameFolder(made.id, to: "Choir", deviceID: phone, now: now)
    #expect(from == "choir" && to == "Choir")
    let names = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(names.contains("Choir") && !names.contains("choir") && !names.contains { $0.hasPrefix("_tmp") })

    var bases = BaseStore(file: FileManager.default.temporaryDirectory.appendingPathComponent("bases-\(UUID().uuidString).json"))
    try bases.set(BaseRecord(id: "A", path: "Choir/A.md", sha256: "x", rev: 1))
    try bases.set(BaseRecord(id: "B", path: "Choir practice/B.md", sha256: "y", rev: 1))
    try bases.rebase(dir: "Choir", to: "Choir 2026")
    #expect(bases.records["A"]?.path == "Choir 2026/A.md")
    #expect(bases.records["B"]?.path == "Choir practice/B.md", "only that folder's notes")
}

@Test func aNoteWithoutItsTranscriptDoesNotMove() throws {
    let (root, desktop) = try vault()
    let local = FileManager.default.temporaryDirectory.appendingPathComponent("local-\(UUID().uuidString)")
    var session = RecordingSession(folder: local, title: "Bench vote", started: now, deviceID: phone, transcriptOnly: true)
    try session.add(text: "We need two more benches.", start: 0.2, end: 2.0)
    try session.finish(at: now.addingTimeInterval(60), duration: 60)
    let writer = VaultWriter(root: root)
    let base = try writer.moveIn(note: session.noteURL, folderID: nil)
    let note = try parseNote(try Data(contentsOf: root.appendingPathComponent(base.path)))
    try FileManager.default.removeItem(at: root.appendingPathComponent(try #require(note.transcript).file))
    #expect(throws: VaultWriteError.sidecarMissing) { try writer.moveNote(base, toFolder: desktop) }
    #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent(base.path).path))
}
