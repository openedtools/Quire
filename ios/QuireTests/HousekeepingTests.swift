// AC-IOS-5 and 8 in the app: housekeeping finalizes a recording cut short, moves finished
// recordings into the vault folder chosen for them, never touches one still in progress, and
// deletes Off audio only once the transcript is in the vault (plan PI13 to PI17).

import Foundation
import RecordingKit
import Synchronization
import Testing
import VaultKit
@testable import Quire

private let device = "dev-0a1b2c3d"

/// Each test has its own record of bases and folder choices (tests run in parallel)
private func isolated() -> (bases: URL, folders: PendingFolders) {
    let name = "quire-test-\(UUID().uuidString)"
    return (FileManager.default.temporaryDirectory.appendingPathComponent("\(name).json"), PendingFolders(defaults: UserDefaults(suiteName: name)!))
}

private func run(_ local: URL, _ vault: URL?, chosen: Bool? = nil, now: Date = Date(), _ store: (bases: URL, folders: PendingFolders)) -> HousekeepingResult {
    housekeep(local: local, vaultRoot: vault, vaultChosen: chosen ?? (vault != nil), now: now, latestSeen: nil, confirmed: [], deviceID: device, basesFile: store.bases, folders: store.folders)
}

private func tempVault(_ name: String) throws -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(name)-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    let now = timestampString(Date())
    let marker = Marker(format: "quire-vault", version: 1, minReader: 1, vaultID: newULID(at: Date()), created: now, createdBy: "dev-desktop1")
    try Data(renderMarker(marker).utf8).write(to: url.appendingPathComponent("quire-format.json"))
    return url
}

private func addFolder(_ name: String, to vault: URL) throws -> String {
    let id = newULID(at: Date())
    let now = timestampString(Date())
    let dir = vault.appendingPathComponent(name)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    try Data(renderFolder(Folder(id: id, name: name, created: now, updated: now, updatedBy: "dev-desktop1")).utf8).write(to: dir.appendingPathComponent("_folder.json"))
    return id
}

private func finished(_ retention: Retention, in local: URL, title: String = "Bench vote") throws -> RecordingSession {
    var session = RecordingSession(folder: local, title: title, started: Date().addingTimeInterval(-120), deviceID: device, transcriptOnly: false, retention: retention)
    try session.add(text: "Two more benches by the pond.", start: 0.5, end: 2.5)
    try session.finish(at: Date().addingTimeInterval(-60), duration: 60)
    try Data("audio".utf8).write(to: session.audioURL!)
    return session
}

@Test func finishedRecordingsMoveIntoTheirFolderAndOffAudioGoesAfterwards() throws {
    let local = try tempVault("local")
    let vault = try tempVault("vault")
    let folderID = try addFolder("Garden Club", to: vault)
    let week = try finished(.d7, in: local)
    let off = try finished(.off, in: local, title: "Quick note")
    let store = isolated()
    store.folders.set(week.id, folderID)

    let result = run(local, vault, store)
    #expect(result.moved == 2 && result.problem == nil)
    let contents = try VaultReader(root: vault).read()
    #expect(contents.meetings.first { $0.id == week.id }?.folderID == folderID)
    #expect(contents.meetings.first { $0.id == off.id }?.folderID == nil)
    #expect(result.report.deleted == [off.id], "Off: deleted once the transcript is in the vault")
    #expect(FileManager.default.fileExists(atPath: week.audioURL!.path), "7 days: still kept")
    #expect(!FileManager.default.fileExists(atPath: off.audioURL!.path))
    #expect(result.phoneIDs.isSuperset(of: [week.id, off.id]), "both stay the phone's own meetings")
    #expect(store.folders.get(week.id) == nil)
    #expect(BaseStore(file: store.bases).records.keys.contains(week.id), "the phone keeps track of what it wrote")
}

@Test func aRecordingInProgressIsNeverMovedOrFinalized() throws {
    let local = try tempVault("local")
    let vault = try tempVault("vault")
    var running = RecordingSession(folder: local, title: "Still going", started: Date(), deviceID: device, transcriptOnly: false, retention: .off)
    try running.add(text: "Hello.", start: 0, end: 1)
    try Data("audio".utf8).write(to: running.audioURL!)
    InProgress.add(running.id)
    defer { InProgress.remove(running.id) }

    let result = run(local, vault, isolated())
    #expect(result.moved == 0)
    let note = try parseNote(try Data(contentsOf: running.noteURL))
    #expect(note.audio.expires == nil, "no deadline while recording")
    #expect(FileManager.default.fileExists(atPath: running.audioURL!.path))
}

@Test func aRecordingCutShortGetsItsDeadlineAndMovesIn() throws {
    let local = try tempVault("local")
    let vault = try tempVault("vault")
    var cut = RecordingSession(folder: local, title: "Cut short", started: Date().addingTimeInterval(-600), deviceID: device, transcriptOnly: false, retention: .h24)
    try cut.add(text: "Where were we.", start: 0, end: 1)
    let now = Date()
    _ = run(local, vault, now: now, isolated())
    let moved = try #require(try VaultReader(root: vault).read().meetings.first { $0.id == cut.id })
    #expect(moved.note.audio.expires == timestampString(now.addingTimeInterval(86_400)))
}

@Test func withoutAVaultRecordingsStayOnThePhoneAndOffStillDeletes() throws {
    let local = try tempVault("local")
    let off = try finished(.off, in: local)
    let result = run(local, nil, isolated())
    #expect(result.moved == 0 && result.report.deleted == [off.id])
    #expect(FileManager.default.fileExists(atPath: off.noteURL.path), "the transcript stays on the phone")
}

@Test func aVaultThatCannotBeWrittenKeepsRecordingsLocalAndSaysSo() throws {
    let local = try tempVault("local")
    let notAVault = FileManager.default.temporaryDirectory.appendingPathComponent("plain-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: notAVault, withIntermediateDirectories: true)
    let week = try finished(.d7, in: local)
    let result = run(local, notAVault, isolated())
    #expect(result.moved == 0 && result.problem != nil)
    #expect(FileManager.default.fileExists(atPath: week.noteURL.path))
}

@Test func offWaitsWhenTheChosenVaultCannotBeReached() throws {
    // The bookmark did not resolve (iCloud signed out, a background launch): the transcript is
    // only on the phone, so its Off audio is kept until it moves in
    let local = try tempVault("local")
    let off = try finished(.off, in: local)
    let result = run(local, nil, chosen: true, isolated())
    #expect(result.report.deleted.isEmpty)
    #expect(FileManager.default.fileExists(atPath: off.audioURL!.path))
}

@Test func aClockThatWentBackGivesNoDeadline() throws {
    let local = try tempVault("local")
    var cut = RecordingSession(folder: local, title: "Cut short", started: Date(), deviceID: device, transcriptOnly: false, retention: .h24)
    try cut.add(text: "Hello.", start: 0, end: 1)
    let store = isolated()
    let result = housekeep(local: local, vaultRoot: nil, vaultChosen: false, now: Date(), latestSeen: Date().addingTimeInterval(86_400), confirmed: [], deviceID: device, basesFile: store.bases, folders: store.folders)
    #expect(result.report.clockWentBack)
    #expect(try parseNote(try Data(contentsOf: cut.noteURL)).audio.expires == nil)
}

// ===== I5: heat during a recording (PI22), with the heat state passed in =====

/// Counts transcriptions, standing in for Parakeet
final class CountingTranscriber: Sendable {
    let calls = Mutex(0)
    var count: Int { calls.withLock { $0 } }
    func transcribe(_ samples: [Float]) async throws -> String {
        calls.withLock { $0 += 1 }
        return "A line."
    }
}

private func speech(at start: Double) -> AudioPiece {
    AudioPiece(start: start, samples: [Float](repeating: 0.1, count: 16_000))
}

@MainActor private func settle() async {
    for _ in 0..<50 { await Task.yield(); try? await Task.sleep(for: .milliseconds(10)) }
}

@MainActor @Test func warmthHoldsThePiecesAndCoolingTranscribesThem() async throws {
    let counter = CountingTranscriber()
    let recorder = Recorder(testTranscribe: { try await counter.transcribe($0) }, folder: try tempVault("heat"))
    await recorder.applyHeat(.serious)
    #expect(recorder.heatHeld && recorder.heatNote == heatHoldNote && recorder.interruptionNote == nil, "the heat note has its own line")
    recorder.testArrive([speech(at: 0), speech(at: 2)])
    await settle()
    #expect(counter.count == 0, "nothing is transcribed while warm")
    #expect(recorder.backlogSeconds >= 2)
    await recorder.applyHeat(.nominal)
    await settle()
    #expect(counter.count == 2 && recorder.lines.count == 2, "both pieces once it cools")
    #expect(recorder.heatNote == nil)
}

@MainActor @Test func atSeriousStopFinishesTheLastLines() async throws {
    let counter = CountingTranscriber()
    let recorder = Recorder(testTranscribe: { try await counter.transcribe($0) }, folder: try tempVault("heat"))
    await recorder.applyHeat(.serious)
    recorder.testArrive([speech(at: 0)])
    await recorder.stop()
    await settle()
    #expect(counter.count == 1 && recorder.phase == .done)
    #expect(recorder.heatNote == nil, "no stale warm note once stopped")
}

@MainActor @Test func atCriticalTheRecordingStopsAndTheRestWaitsToCool() async throws {
    let counter = CountingTranscriber()
    let recorder = Recorder(testTranscribe: { try await counter.transcribe($0) }, folder: try tempVault("heat"))
    await recorder.applyHeat(.serious)
    recorder.testArrive([speech(at: 0), speech(at: 2)])
    await recorder.applyHeat(.critical)
    await settle()
    #expect(recorder.phase == .finishing && counter.count == 0, "stopped, and nothing transcribed at critical")
    #expect(recorder.interruptionNote == heatStopNote && recorder.heatNote == heatFinishNote)
    await recorder.applyHeat(.fair)
    await settle()
    #expect(counter.count == 2 && recorder.phase == .done, "finished once it cools")
}

@MainActor @Test func aLongHoldStopsTheRecordingInsteadOfGrowing() async throws {
    let counter = CountingTranscriber()
    let recorder = Recorder(testTranscribe: { try await counter.transcribe($0) }, folder: try tempVault("heat"))
    await recorder.applyHeat(.serious)
    // Half an hour of held speech, in 15-second pieces of 16-bit samples
    recorder.testArrive((0..<120).map { AudioPiece(start: Double($0) * 15, samples: [Float](repeating: 0.1, count: 16_000 * 15)) })
    await settle()
    #expect(recorder.phase == .finishing || recorder.phase == .done)
    #expect(recorder.interruptionNote == heatLongNote)
}

@Test func heldPiecesKeepTheirTimesAndSound() {
    let held = HeldPiece(AudioPiece(start: 4, samples: [0, 0.5, -0.5, 1, -1]))
    #expect(held.start == 4 && abs(held.end - (4 + 5.0 / 16_000)) < 1e-9)
    #expect(zip(held.samples, [0, 0.5, -0.5, 1, -1]).allSatisfy { abs($0 - $1) < 0.0001 })
}
