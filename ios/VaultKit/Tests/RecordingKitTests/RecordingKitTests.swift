// Pause detection and saving as it goes (plan I2, F-IOS-REC-5, F-IOS-PRIV-2).

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private func tone(_ seconds: Double, amplitude: Float = 0.2) -> [Float] {
    (0..<Int(seconds * 16_000)).map { amplitude * sin(Float($0) * 2 * .pi * 220 / 16_000) }
}

private func quiet(_ seconds: Double) -> [Float] { [Float](repeating: 0.0005, count: Int(seconds * 16_000)) }

@Test func piecesEndAtPausesWithTrueTimes() {
    var seg = PauseSegmenter()
    var pieces = seg.push(quiet(0.5) + tone(2) + quiet(1) + tone(1.5) + quiet(1))
    if let rest = seg.flush() { pieces.append(rest) }
    #expect(pieces.count == 2)
    #expect(abs(pieces[0].start - 0.0) < 0.05, "the first piece starts where the recording does")
    #expect(pieces[0].end > 2.4 && pieces[0].end < 3.2)
    #expect(pieces[1].start > 2.4 && pieces[1].start < 3.2)
}

@Test func aLongMonologueIsCutSoLinesKeepComing() {
    var seg = PauseSegmenter()
    let pieces = seg.push(tone(30))
    #expect(pieces.count == 2)
    #expect(pieces.allSatisfy { $0.end - $0.start <= 14.01 })
}

@Test func silenceAloneProducesNothing() {
    var seg = PauseSegmenter()
    #expect(seg.push(quiet(40)).isEmpty)
    #expect(seg.flush() == nil)
}

@Test func afterAPauseTimesContinueFromTheRecordedAudio() {
    // Paused time is not recorded, so times count recorded audio only (like the audio file)
    var seg = PauseSegmenter()
    _ = seg.push(tone(2))
    _ = seg.flush()
    var pieces = seg.push(tone(2) + quiet(1))
    if let rest = seg.flush() { pieces.append(rest) }
    #expect(pieces.first.map { abs($0.start - 2.0) < 0.05 } == true)
}

@Test func anHourOfAudioIsCutInLinearTime() {
    var seg = PauseSegmenter()
    let minute = tone(1.5) + quiet(1) // a 2.5 s pattern
    var hour: [Float] = []
    hour.reserveCapacity(16_000 * 3600)
    for _ in 0..<1440 { hour += minute }
    let clock = ContinuousClock()
    let took = clock.measure { _ = seg.push(hour) }
    #expect(took < .seconds(20), "an hour of audio took \(took)")
}

private func folder() -> URL { FileManager.default.temporaryDirectory.appendingPathComponent("rec-\(UUID().uuidString)") }

@Test func everyLineIsSavedAsAValidNoteAndSidecar() throws {
    let dir = folder()
    var session = RecordingSession(folder: dir, title: "Garden club walk", started: Date(timeIntervalSince1970: 1_790_000_000), deviceID: "dev-0a1b2c3d", transcriptOnly: false)
    try session.add(text: "We need more benches.", start: 0.4, end: 2.1)
    try session.bookmark(at: 2.5)
    try session.add(text: "  And a tap.  ", start: 3.0, end: 4.2)

    // Read back exactly as the desktop and the vault reader would
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(isULID(note.id) && note.id == session.id)
    #expect(note.source == "recorded" && note.mode == "audio" && note.updatedBy == "dev-0a1b2c3d")
    #expect(note.body == "\n## Notes\n\n- Bookmark at 00:02\n")
    let ref = try #require(note.transcript)
    let side = try Data(contentsOf: dir.appendingPathComponent(ref.file))
    #expect(sha256Hex(side) == ref.sha256, "the note points at the sidecar it was written with")
    let (_, segs) = try parseSidecar(side)
    #expect(segs.map(\.text) == ["We need more benches.", "And a tap."])
    let files = try FileManager.default.contentsOfDirectory(atPath: dir.path)
    #expect(files.filter { $0.hasSuffix(".transcript.json") }.count == 1, "the previous sidecar is removed")
    #expect(note.rev >= 3, "each save is a new revision")
}

@Test func aFailedNoteWriteLeavesThePreviousPairReadable() throws {
    let dir = folder()
    var session = RecordingSession(folder: dir, title: "Garden club walk", started: Date(), deviceID: "dev-1", transcriptOnly: true)
    try session.add(text: "First line.", start: 0, end: 1)
    // The new sidecar can be written but the note cannot be replaced (as on a full disk)
    try FileManager.default.setAttributes([.immutable: true], ofItemAtPath: session.noteURL.path)
    #expect(throws: (any Error).self) { try session.add(text: "Second line.", start: 1, end: 2) }
    try FileManager.default.setAttributes([.immutable: false], ofItemAtPath: session.noteURL.path)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    let ref = try #require(note.transcript)
    let side = try Data(contentsOf: dir.appendingPathComponent(ref.file))
    #expect(sha256Hex(side) == ref.sha256, "the note still points at a sidecar that exists")
}

@Test func anImportHasNoRecordingTimes() throws {
    let dir = folder()
    var session = RecordingSession(folder: dir, title: "Voice memo", started: Date(timeIntervalSince1970: 1_790_000_000), deviceID: "dev-1", transcriptOnly: false, imported: true)
    try session.add(text: "Hello.", start: 0, end: 1)
    try session.finish(at: Date(timeIntervalSince1970: 1_790_000_060), duration: 60)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(note.source == "imported")
    #expect(note.started == nil && note.ended == nil && note.durationS == nil)
}

@Test func transcriptOnlyNeverHasAnAudioFile() throws {
    let dir = folder()
    var session = RecordingSession(folder: dir, title: "Private", started: Date(), deviceID: "dev-1", transcriptOnly: true)
    try session.add(text: "Hello.", start: 0, end: 1)
    try session.finish(at: Date(), duration: 1)
    #expect(session.audioURL == nil)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(note.mode == "transcript_only")
    #expect(note.audio.state == "none")
    #expect(try FileManager.default.contentsOfDirectory(atPath: dir.path).allSatisfy { !$0.hasSuffix(".m4a") && !$0.hasSuffix(".\(RecordingAudio.fileExtension)") })
}

@Test func ulidsCarryTheirTimeAndAreValid() {
    let date = Date(timeIntervalSince1970: 1_790_000_000)
    let a = newULID(at: date)
    let b = newULID(at: date.addingTimeInterval(1))
    #expect(isULID(a) && isULID(b))
    #expect(a.prefix(10) < b.prefix(10), "later times sort later")
    #expect(newULID(at: date).prefix(10) == a.prefix(10))
}
