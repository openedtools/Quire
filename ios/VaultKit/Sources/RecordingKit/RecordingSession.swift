// A recording on the phone, saved in the vault format as it goes (plan PI10): the note and its
// sidecar are rewritten after every new line, atomically, so a crash or a killed app loses only
// the seconds not yet transcribed (F-IOS-REC-5). Writes only into the app's own folder; I3 moves
// finished recordings into the iCloud vault.

import Foundation
import VaultKit

public struct RecordingSession: Sendable {
    public let id: String
    public let folder: URL
    public var title: String
    public let started: Date
    public let deviceID: String
    /// `audio` or `transcript_only`: transcript only never writes an audio file (F-IOS-PRIV-2)
    public let mode: String
    /// `recorded` from the microphone or `imported` from a file, which has no recording times (section 6)
    public let source: String
    /// The retention chosen when the recording started (F-RET-2); the deadline is set when it ends
    public let retention: Retention
    /// `thoughts` for a recording of one's own thoughts (PN4), nil for a meeting
    public let kind: String?
    public private(set) var segments: [Segment] = []
    public private(set) var bookmarks: [Double] = []
    public private(set) var rev: Int64 = 0
    public private(set) var ended: Date?
    public private(set) var duration: Double = 0
    private var sidecarFile: String?

    public init(folder: URL, title: String, started: Date, deviceID: String, transcriptOnly: Bool, imported: Bool = false, retention: Retention = .keep, id: String? = nil, kind: String? = nil) {
        self.id = id ?? newULID(at: started)
        self.kind = kind
        self.folder = folder
        self.title = title
        self.started = started
        self.deviceID = deviceID
        self.mode = transcriptOnly ? "transcript_only" : "audio"
        self.source = imported ? "imported" : "recorded"
        self.retention = retention
    }

    public var noteURL: URL { folder.appendingPathComponent("\(id).md") }
    /// The audio file, in the app's own storage; never in the vault (PI9)
    public var audioURL: URL? { mode == "audio" ? folder.appendingPathComponent("\(id).\(RecordingAudio.fileExtension)") : nil }

    public mutating func add(text: String, start: Double, end: Double) throws {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        segments.append(Segment(id: "seg-\(segments.count + 1)", start: start, end: end, speaker: nil, text: clean))
        duration = max(duration, end)
        try save()
    }

    /// A bookmark is a line in the notes with its time (PI11).
    public mutating func bookmark(at seconds: Double) throws {
        bookmarks.append(seconds)
        try save()
    }

    public mutating func finish(at date: Date, duration: Double) throws {
        ended = date
        self.duration = max(self.duration, duration)
        try save()
    }

    static func clock(_ seconds: Double) -> String {
        let s = Int(max(0, min(seconds, 86_400)))
        return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60) : String(format: "%02d:%02d", s / 60, s % 60)
    }

    public var body: String {
        guard !bookmarks.isEmpty else { return "" }
        return "\n## Notes\n\n" + bookmarks.map { "- Bookmark at \(Self.clock($0))" }.joined(separator: "\n") + "\n"
    }

    public func note(sidecar: TranscriptRef?, rev: Int64? = nil) -> Note {
        let rev = rev ?? self.rev
        let started = timestampString(self.started)
        let end = ended.map(timestampString) ?? timestampString(self.started.addingTimeInterval(duration))
        let recorded = source == "recorded"
        return Note(
            id: id, title: title, created: started, updated: end, updatedBy: deviceID, rev: max(rev, 1),
            started: recorded ? started : nil, ended: recorded ? end : nil, durationS: recorded ? Int64(duration.rounded()) : nil,
            language: nil, source: source, mode: mode,
            speakers: [], transcript: sidecar, summary: nil,
            // The deadline counts from the end, so it is written only once the recording has ended
            audio: AudioMeta(
                retention: retention.rawValue, expires: mode == "audio" ? ended.flatMap(retention.expires).map(timestampString) : nil,
                state: mode == "audio" ? "present" : "none", deletedAt: nil),
            extra: kind.map { [("kind", .str($0))] } ?? [], body: body)
    }

    /// Writes the sidecar (new name for new content), then the note, and only then removes the
    /// old sidecar: the vault's commit order, so the note on disk always points at a file that exists.
    public mutating func save() throws {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var ref: TranscriptRef?
        var name: String?
        if !segments.isEmpty {
            let data = Data(renderSidecar(noteID: id, segments: segments).utf8)
            let sha = sha256Hex(data)
            let file = sidecarName(noteID: id, sha256: sha)
            try data.write(to: folder.appendingPathComponent(file), options: .atomic)
            name = file
            ref = TranscriptRef(file: file, sha256: sha, segments: Int64(segments.count))
        }
        try Data(renderNote(note(sidecar: ref, rev: rev + 1)).utf8).write(to: noteURL, options: .atomic)
        rev += 1
        if let old = sidecarFile, old != name {
            try? FileManager.default.removeItem(at: folder.appendingPathComponent(old))
        }
        sidecarFile = name
    }
}

/// UTC with milliseconds and a trailing Z (sync-format.md section 3).
public func timestampString(_ date: Date) -> String {
    let style = Date.ISO8601FormatStyle(includingFractionalSeconds: true, timeZone: .gmt)
    return date.formatted(style)
}

private let crockfordDigits = Array("0123456789ABCDEFGHJKMNPQRSTVWXYZ")

/// A ULID whose time part is `date` (section 3).
public func newULID(at date: Date) -> String {
    let millis = UInt64(max(0, date.timeIntervalSince1970 * 1000))
    var value: (UInt64, UInt64) = (millis & 0xFFFF_FFFF_FFFF, 0)
    var random = SystemRandomNumberGenerator()
    let r1 = UInt64.random(in: 0...UInt64.max, using: &random)
    let r2 = UInt64.random(in: 0...0xFFFF, using: &random)
    // 128 bits: 48 time bits, then 80 random bits
    let high = (value.0 << 16) | r2
    value = (high, r1)
    var out: [Character] = []
    for i in (0..<26).reversed() {
        let shift = 5 * i
        let digit: UInt64
        if shift >= 64 {
            digit = (value.0 >> UInt64(shift - 64)) & 31
        } else if shift > 59 {
            digit = ((value.1 >> UInt64(shift)) | (value.0 << UInt64(64 - shift))) & 31
        } else {
            digit = (value.1 >> UInt64(shift)) & 31
        }
        out.append(crockfordDigits[Int(digit)])
    }
    return String(out)
}
