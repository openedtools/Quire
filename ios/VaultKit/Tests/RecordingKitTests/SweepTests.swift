// AC-IOS-5: the sweep deletes the phone's audio, and any vault copy, exactly when the note says,
// verifies it is gone, never touches a file it cannot tie to a note, asks before deleting many,
// and pauses when the clock went back (spec F-RET-4 to 7, 13; sync-format.md section 10).

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private let device = "dev-0a1b2c3d"
private let day: TimeInterval = 86_400
private let ended = Date(timeIntervalSince1970: 1_790_000_000)

private func tempDir(_ name: String) -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(name)-\(UUID().uuidString)")
    try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

/// A finished recording's note with the given retention, and its audio file in `folder`
private func recording(_ retention: Retention, in folder: URL, transcriptOnly: Bool = false) throws -> Note {
    var session = RecordingSession(folder: folder, title: "Walk", started: ended.addingTimeInterval(-60), deviceID: device, transcriptOnly: transcriptOnly, retention: retention)
    try session.finish(at: ended, duration: 60)
    if let audio = session.audioURL { try Data("audio".utf8).write(to: audio) }
    return session.note(sidecar: nil)
}

private func exists(_ folder: URL, _ id: String, _ ext: String = RecordingAudio.fileExtension) -> Bool {
    FileManager.default.fileExists(atPath: folder.appendingPathComponent("\(id).\(ext)").path)
}

@Test func audioGoesAtItsDeadlineAndNotBefore() throws {
    let local = tempDir("local")
    let week = try recording(.d7, in: local)
    let keep = try recording(.keep, in: local)
    let sweeper = AudioSweeper(localFolder: local, vaultRoot: nil)

    var report = sweeper.sweep(localNotes: [week, keep], vaultNotes: [], now: ended.addingTimeInterval(7 * day - 1), latestSeen: nil)
    #expect(report.deleted.isEmpty && exists(local, week.id))

    report = sweeper.sweep(localNotes: [week, keep], vaultNotes: [], now: ended.addingTimeInterval(7 * day), latestSeen: nil)
    #expect(report.deleted == [week.id])
    #expect(!exists(local, week.id), "deleted and checked gone")
    #expect(exists(local, keep.id))
}

@Test func offWaitsForTheVaultWhenOneIsChosen() throws {
    // PI15: Off deletes once the transcript is confirmed in the vault, or saved locally with no vault
    let local = tempDir("local")
    let off = try recording(.off, in: local)
    let withVault = AudioSweeper(localFolder: local, vaultRoot: tempDir("vault"))
    #expect(withVault.sweep(localNotes: [off], vaultNotes: [], now: ended, latestSeen: nil).deleted.isEmpty, "still only on the phone")
    #expect(exists(local, off.id))
    #expect(withVault.sweep(localNotes: [], vaultNotes: [off], writtenToVault: [off.id], now: ended, latestSeen: nil).deleted == [off.id], "written and read back: gone")

    let other = try recording(.off, in: local)
    let noVault = AudioSweeper(localFolder: local, vaultRoot: nil)
    #expect(noVault.sweep(localNotes: [other], vaultNotes: [], now: ended, latestSeen: nil).deleted == [other.id])
}

@Test func deleteNowIsAbsorbingWhateverTheDeadline() throws {
    let local = tempDir("local")
    var note = try recording(.keep, in: local)
    deleteAudioNow(&note, now: ended, deviceID: device)
    let report = AudioSweeper(localFolder: local, vaultRoot: nil).sweep(localNotes: [note], vaultNotes: [], now: ended, latestSeen: nil)
    #expect(report.deleted == [note.id] && !exists(local, note.id))
}

@Test func aFileWithNoNoteIsNeverTouched() throws {
    // Unmanaged audio is listed, never deleted automatically (F-RET-2)
    let local = tempDir("local")
    let stray = newULID(at: ended)
    try Data("audio".utf8).write(to: local.appendingPathComponent("\(stray).m4a"))
    try Data("notes".utf8).write(to: local.appendingPathComponent("shopping.txt"))
    let report = AudioSweeper(localFolder: local, vaultRoot: nil).sweep(localNotes: [], vaultNotes: [], now: ended.addingTimeInterval(400 * day), latestSeen: nil)
    #expect(report.deleted.isEmpty && report.unmanaged == 1)
    #expect(exists(local, stray, "m4a") && FileManager.default.fileExists(atPath: local.appendingPathComponent("shopping.txt").path))
}

@Test func aVaultCopyOfExpiredAudioIsRemovedEvenWithAudioSyncOff() throws {
    // Section 10, rule 3: any device that finds `_audio/<id>.*` for an expired note removes it
    let local = tempDir("local")
    let vault = tempDir("vault")
    let audioDir = vault.appendingPathComponent("_audio")
    try FileManager.default.createDirectory(at: audioDir, withIntermediateDirectories: true)
    let desktopNote = try recording(.h24, in: tempDir("desktop"))
    try Data("audio".utf8).write(to: audioDir.appendingPathComponent("\(desktopNote.id).m4a"))
    let report = AudioSweeper(localFolder: local, vaultRoot: vault).sweep(localNotes: [], vaultNotes: [desktopNote], now: ended.addingTimeInterval(day), latestSeen: nil)
    #expect(report.deleted == [desktopNote.id])
    #expect(!exists(audioDir, desktopNote.id, "m4a"))
}

@Test func aLinkIsNeverFollowedOrRemoved() throws {
    let local = tempDir("local")
    let elsewhere = tempDir("elsewhere").appendingPathComponent("keep-me.m4a")
    try Data("someone's file".utf8).write(to: elsewhere)
    var note = try recording(.off, in: tempDir("other"))
    deleteAudioNow(&note, now: ended, deviceID: device)
    try FileManager.default.createSymbolicLink(at: local.appendingPathComponent("\(note.id).m4a"), withDestinationURL: elsewhere)
    let report = AudioSweeper(localFolder: local, vaultRoot: nil).sweep(localNotes: [note], vaultNotes: [], now: ended, latestSeen: nil)
    #expect(report.deleted.isEmpty)
    #expect(FileManager.default.fileExists(atPath: elsewhere.path))
}

@Test func manyAtOnceWaitsForConfirmationAndABackwardsClockPauses() throws {
    let local = tempDir("local")
    let notes = try (0..<5).map { _ in try recording(.h24, in: local) }
    let sweeper = AudioSweeper(localFolder: local, vaultRoot: nil)
    let later = ended.addingTimeInterval(2 * day)

    var report = sweeper.sweep(localNotes: notes, vaultNotes: [], now: later, latestSeen: nil)
    #expect(report.heldBack == notes.map(\.id).sorted() && report.deleted.isEmpty, "5 of 5 is more than a quarter")
    #expect(notes.allSatisfy { exists(local, $0.id) })

    report = sweeper.sweep(localNotes: notes, vaultNotes: [], now: later, latestSeen: later.addingTimeInterval(3600), confirmed: Set(report.heldBack))
    #expect(report.clockWentBack && report.deleted.isEmpty, "a clock earlier than one seen pauses deletion")

    report = sweeper.sweep(localNotes: notes, vaultNotes: [], now: later, latestSeen: nil, confirmed: Set(notes.map(\.id)))
    #expect(report.deleted.count == 5)
}

@Test func aYesCoversOnlyWhatItWasAskedAbout() throws {
    // As on the desktop: recordings that became due after the question are asked about again
    let local = tempDir("local")
    let asked = try (0..<5).map { _ in try recording(.h24, in: local) }
    let sweeper = AudioSweeper(localFolder: local, vaultRoot: nil)
    let first = sweeper.sweep(localNotes: asked, vaultNotes: [], now: ended.addingTimeInterval(2 * day), latestSeen: nil)
    let later = try (0..<5).map { _ in try recording(.h24, in: local) }
    let report = sweeper.sweep(localNotes: asked + later, vaultNotes: [], now: ended.addingTimeInterval(2 * day), latestSeen: nil, confirmed: Set(first.heldBack))
    #expect(Set(report.deleted) == Set(asked.map(\.id)))
    #expect(Set(report.heldBack) == Set(later.map(\.id)))
    #expect(later.allSatisfy { exists(local, $0.id) })
}

@Test func offWaitsWhileTheChosenVaultCannotBeReached() throws {
    // iCloud signed out, or a background launch before the vault is available
    let local = tempDir("local")
    let off = try recording(.off, in: local)
    let report = AudioSweeper(localFolder: local, vaultRoot: nil, vaultChosen: true).sweep(localNotes: [off], vaultNotes: [], now: ended, latestSeen: nil)
    #expect(report.deleted.isEmpty && exists(local, off.id))
}

@Test func whileThePhoneHasItsOwnCopyThatCopyDecides() throws {
    // A move cut short left an older copy in the vault; the person then chose Keep this one
    let local = tempDir("local")
    var note = try recording(.h24, in: local)
    let older = note
    keepAudio(&note, now: ended, deviceID: device)
    let report = AudioSweeper(localFolder: local, vaultRoot: tempDir("vault")).sweep(localNotes: [note], vaultNotes: [older], now: ended.addingTimeInterval(3 * day), latestSeen: nil)
    #expect(report.deleted.isEmpty && exists(local, note.id))
}

@Test func offIsNotConfirmedBySomeoneElsesNoteWithTheSameID() throws {
    let local = tempDir("local")
    let off = try recording(.off, in: local)
    var stranger = off
    stranger.updatedBy = "dev-ffffffff"
    let report = AudioSweeper(localFolder: local, vaultRoot: tempDir("vault")).sweep(localNotes: [], vaultNotes: [stranger], now: ended, latestSeen: nil)
    #expect(report.deleted.isEmpty, "only the phone's own read-back write confirms Off")
}

@Test func transcriptOnlyAndRecordingsInProgressAreLeftAlone() throws {
    let only = try recording(.off, in: tempDir("only"), transcriptOnly: true)
    #expect(audioStatus(only.audio, ended: only.ended, now: ended.addingTimeInterval(30 * day)) == .none, "transcript only has no audio to delete")

    let local = tempDir("local")
    let session = RecordingSession(folder: local, title: "Still going", started: ended, deviceID: device, transcriptOnly: false, retention: .off)
    try Data("audio".utf8).write(to: session.audioURL!)
    let report = AudioSweeper(localFolder: local, vaultRoot: nil).sweep(localNotes: [session.note(sidecar: nil)], vaultNotes: [], now: ended.addingTimeInterval(30 * day), latestSeen: nil)
    #expect(report.deleted.isEmpty, "no deadline until the recording ends")
    #expect(exists(local, session.id))
}
