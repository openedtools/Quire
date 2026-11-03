// AC-IOS-5: the desktop's retention rules on the phone (spec F-RET-1, 2, 13, sync-format.md
// section 10), with the clock passed in. Mirrors the desktop's `retention/policy.rs` tests.

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private func t(_ day: Int, _ hour: Int) -> Date {
    var c = DateComponents()
    (c.year, c.month, c.day, c.hour) = (2026, 11, day, hour)
    c.timeZone = TimeZone(identifier: "UTC")
    return Calendar(identifier: .gregorian).date(from: c)!
}

private func audio(_ retention: Retention, ended: Date, state: String = "present") -> AudioMeta {
    AudioMeta(retention: retention.rawValue, expires: retention.expires(ended: ended).map(timestampString), state: state, deletedAt: nil)
}

@Test func eachSettingIsDueExactlyAtItsDeadlineAndNotAMillisecondBefore() {
    let ended = t(2, 14)
    for (r, period) in [(Retention.off, 0.0), (.h24, 86_400), (.d7, 7 * 86_400), (.d14, 14 * 86_400), (.d30, 30 * 86_400)] {
        let a = audio(r, ended: ended)
        let deadline = ended.addingTimeInterval(period)
        #expect(audioStatus(a, ended: timestampString(ended), now: deadline) == .expired(at: deadline), "\(r) at the deadline")
        #expect(audioStatus(a, ended: timestampString(ended), now: deadline.addingTimeInterval(-0.001)) == .kept(until: deadline), "\(r) 1 ms before")
    }
    let keep = audio(.keep, ended: ended)
    #expect(keep.expires == nil)
    #expect(audioStatus(keep, ended: timestampString(ended), now: ended.addingTimeInterval(10_000 * 86_400)) == .kept(until: nil))
}

@Test func deletedAndAudioLessNotesAreNeverDue() {
    var a = audio(.off, ended: t(2, 14), state: "deleted")
    a.deletedAt = timestampString(t(3, 9))
    #expect(audioStatus(a, ended: nil, now: t(30, 0)) == .deleted(at: t(3, 9)))
    #expect(audioStatus(audio(.off, ended: t(2, 14), state: "none"), ended: nil, now: t(30, 0)) == .none)
}

@Test func anInvalidOrUnreadableDeadlineMeansKeep() {
    // Section 10, rule 4: `expires` earlier than `ended`, or not a timestamp, is treated as keep
    var a = audio(.d7, ended: t(2, 14))
    a.expires = timestampString(t(1, 0))
    #expect(audioStatus(a, ended: timestampString(t(2, 14)), now: t(30, 0)) == .kept(until: nil))
    a.expires = "next tuesday"
    #expect(audioStatus(a, ended: timestampString(t(2, 14)), now: t(30, 0)) == .kept(until: nil))
    a.expires = "2026-11-09T14:00:00Z" // no milliseconds: not the format
    #expect(audioStatus(a, ended: nil, now: t(30, 0)) == .kept(until: nil))
}

@Test func storedNamesRoundTripAndMatchTheDesktop() {
    #expect(Retention.allCases.map(\.rawValue) == ["off", "h24", "d7", "d14", "d30", "keep"])
    #expect(Retention(rawValue: "forever") == nil)
    #expect(Retention.standard == .d7, "the desktop's default for new installs (D7)")
}

@Test func massExpiryAndTheClockGuard() {
    #expect(needsConfirmation(due: 21, withAudio: 1000))
    #expect(!needsConfirmation(due: 20, withAudio: 1000))
    #expect(needsConfirmation(due: 5, withAudio: 16), "more than a quarter")
    #expect(!needsConfirmation(due: 4, withAudio: 16), "exactly a quarter")
    #expect(!needsConfirmation(due: 3, withAudio: 4), "small libraries are not asked about a few recordings")
    #expect(clockWentBack(now: t(2, 10), latestSeen: t(2, 11)))
    #expect(!clockWentBack(now: t(2, 11), latestSeen: t(2, 11)))
    #expect(!clockWentBack(now: t(2, 11).addingTimeInterval(-240), latestSeen: t(2, 11)), "five minutes of slack")
    #expect(!clockWentBack(now: t(2, 11), latestSeen: nil))
}

@Test func timestampsParseOnlyInTheVaultFormat() {
    #expect(parseTimestamp("2026-11-02T14:00:00.000Z") == t(2, 14))
    let text = timestampString(t(5, 3).addingTimeInterval(0.123))
    #expect(parseTimestamp(text).map(timestampString) == text)
    #expect(parseTimestamp("2026-11-02 14:00:00") == nil)
    #expect(parseTimestamp("2026-11-02T14:00:00.000+01:00") == nil)
}

// ===== The recording's own fields (F-RET-2) =====

private func folder() -> URL { FileManager.default.temporaryDirectory.appendingPathComponent("ret-\(UUID().uuidString)") }

@Test func aFinishedRecordingCarriesItsDeadline() throws {
    let ended = t(2, 15)
    var session = RecordingSession(folder: folder(), title: "Garden club walk", started: t(2, 14), deviceID: "dev-1", transcriptOnly: false, retention: .d14)
    #expect(session.note(sidecar: nil).audio.expires == nil, "no deadline while recording")
    try session.finish(at: ended, duration: 3600)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(note.audio.retention == "d14" && note.audio.state == "present")
    #expect(note.audio.expires == timestampString(ended.addingTimeInterval(14 * 86_400)))
    #expect(isFinalized(note))
}

@Test func transcriptOnlyHasNoDeadline() throws {
    var session = RecordingSession(folder: folder(), title: "Private", started: t(2, 14), deviceID: "dev-1", transcriptOnly: true, retention: .d7)
    try session.finish(at: t(2, 15), duration: 60)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(note.audio.state == "none" && note.audio.expires == nil)
}

@Test func aRecordingCutShortIsFinalizedWhenFound() throws {
    // The app was ended mid-recording: the note has no deadline yet; it gets one from the time it is found
    let session = RecordingSession(folder: folder(), title: "Cut short", started: t(2, 14), deviceID: "dev-1", transcriptOnly: false, retention: .h24)
    var note = session.note(sidecar: nil)
    #expect(!isFinalized(note))
    finalizeRecovered(&note, now: t(3, 9), deviceID: "dev-1")
    #expect(note.audio.expires == timestampString(t(4, 9)))
    #expect(isFinalized(note))
    var kept = RecordingSession(folder: folder(), title: "Kept", started: t(2, 14), deviceID: "dev-1", transcriptOnly: false, retention: .keep).note(sidecar: nil)
    #expect(isFinalized(kept), "keep needs no deadline")
    finalizeRecovered(&kept, now: t(3, 9), deviceID: "dev-1")
    #expect(kept.audio.expires == nil)
}

@Test func keepThisOneAndDeleteNowChangeTheNoteAsTheFormatSays() {
    var note = RecordingSession(folder: folder(), title: "Walk", started: t(2, 14), deviceID: "dev-1", transcriptOnly: false, retention: .d7).note(sidecar: nil)
    finalizeRecovered(&note, now: t(2, 15), deviceID: "dev-1")
    #expect(keepAudio(&note, now: t(3, 0), deviceID: "dev-2"))
    #expect(note.audio.retention == "keep" && note.audio.expires == nil && note.updatedBy == "dev-2")
    deleteAudioNow(&note, now: t(4, 0), deviceID: "dev-2")
    #expect(note.audio.state == "deleted" && note.audio.deletedAt == timestampString(t(4, 0)))
    #expect(audioStatus(note.audio, ended: note.ended, now: t(4, 0)) == .deleted(at: t(4, 0)))
}

@Test func keepingIsRefusedOnceTheAudioHasExpired() {
    // Section 10, rule 1: extending is possible only before expiry
    var note = RecordingSession(folder: folder(), title: "Walk", started: t(2, 14), deviceID: "dev-1", transcriptOnly: false, retention: .h24).note(sidecar: nil)
    finalizeRecovered(&note, now: t(2, 15), deviceID: "dev-1")
    let before = note
    #expect(!keepAudio(&note, now: t(4, 0), deviceID: "dev-1"))
    #expect(note == before)
}
