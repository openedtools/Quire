// Audio retention on the phone, the desktop's rules exactly (spec F-RET-1, 2, 13 and
// sync-format.md section 10): the six settings, the deadline in the note, the effective state
// every device computes without writing it back, the mass-expiry guard and the clock check.
// No files here; the clock is passed in.

import Foundation
import VaultKit

public enum Retention: String, CaseIterable, Sendable {
    case off, h24, d7, d14, d30, keep

    /// The desktop's default for new installs (decision D7)
    public static let standard = Retention.d7

    /// How long audio lives after the recording ends; nil for keep
    public var period: TimeInterval? {
        switch self {
        case .off: 0
        case .h24: 86_400
        case .d7: 7 * 86_400
        case .d14: 14 * 86_400
        case .d30: 30 * 86_400
        case .keep: nil
        }
    }

    public func expires(ended: Date) -> Date? { period.map { ended.addingTimeInterval($0) } }

    public var label: String {
        switch self {
        case .off: "Off"
        case .h24: "24 hours"
        case .d7: "7 days"
        case .d14: "14 days"
        case .d30: "30 days"
        case .keep: "Keep"
        }
    }

    /// The plain-language line under each choice, as on the desktop
    public var explanation: String {
        switch self {
        case .off: "Audio is deleted as soon as the transcript is safely saved."
        case .keep: "Audio is kept until you delete it."
        default: "Audio is deleted \(label) after the recording ends. The transcript stays."
        }
    }
}

/// What a note's audio is now, computed from the note and the clock (section 10).
public enum AudioStatus: Equatable, Sendable {
    /// Never written (transcript only)
    case none
    /// Kept; `until` is the deadline, nil for keep (or a deadline that is not valid)
    case kept(until: Date?)
    /// Past its deadline: every device deletes its copy
    case expired(at: Date)
    /// Deleted on purpose ("Delete audio now" or deleting the meeting)
    case deleted(at: Date?)
}

/// A vault timestamp (UTC, milliseconds, a trailing Z, section 3), or nil for anything else.
public func parseTimestamp(_ text: String) -> Date? {
    guard text.wholeMatch(of: /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z/) != nil else { return nil }
    return try? Date(text, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true, timeZone: .gmt))
}

public func audioStatus(_ audio: AudioMeta, ended: String?, now: Date) -> AudioStatus {
    switch audio.state {
    case "deleted": return .deleted(at: audio.deletedAt.flatMap(parseTimestamp))
    case "present": break
    default: return .none
    }
    // An unreadable deadline, or one earlier than the end, is treated as keep (rule 4)
    guard let text = audio.expires, let expires = parseTimestamp(text) else { return .kept(until: nil) }
    if let end = ended.flatMap(parseTimestamp), expires < end { return .kept(until: nil) }
    return now >= expires ? .expired(at: expires) : .kept(until: expires)
}

/// More than 20 recordings at once, or more than a quarter of all recordings with audio, needs
/// the person's confirmation (F-RET-13). The quarter rule applies from 4 recordings due.
public func needsConfirmation(due: Int, withAudio: Int) -> Bool {
    due > 20 || (due >= 4 && due * 4 > withAudio)
}

/// A clock earlier than one already seen (by more than five minutes of slack) pauses deletion.
public func clockWentBack(now: Date, latestSeen: Date?) -> Bool {
    latestSeen.map { now.addingTimeInterval(300) < $0 } ?? false
}

/// True when the recording has its deadline (or needs none); false for one cut short mid-recording.
public func isFinalized(_ note: Note) -> Bool {
    guard note.audio.state == "present", let retention = Retention(rawValue: note.audio.retention), retention != .keep else { return true }
    return note.audio.expires != nil
}

private func touch(_ note: inout Note, now: Date, deviceID: String) {
    note.rev += 1
    note.updated = timestampString(now)
    note.updatedBy = deviceID
}

/// A recording the app was ended in the middle of gets its deadline from the time it is found
/// (F-RET-2: recovered recordings use the time of recovery).
public func finalizeRecovered(_ note: inout Note, now: Date, deviceID: String) {
    guard !isFinalized(note), let retention = Retention(rawValue: note.audio.retention) else { return }
    note.audio.expires = retention.expires(ended: now).map(timestampString)
    touch(&note, now: now, deviceID: deviceID)
}

/// "Keep this one": possible only while the audio is still kept (section 10, rule 1).
@discardableResult
public func keepAudio(_ note: inout Note, now: Date, deviceID: String) -> Bool {
    guard case .kept = audioStatus(note.audio, ended: note.ended, now: now) else { return false }
    note.audio.retention = Retention.keep.rawValue
    note.audio.expires = nil
    touch(&note, now: now, deviceID: deviceID)
    return true
}

/// "Delete audio now": absorbing, whatever the deadline (section 10, rule 2).
public func deleteAudioNow(_ note: inout Note, now: Date, deviceID: String) {
    guard note.audio.state == "present" else { return }
    note.audio.state = "deleted"
    note.audio.deletedAt = timestampString(now)
    touch(&note, now: now, deviceID: deviceID)
}
