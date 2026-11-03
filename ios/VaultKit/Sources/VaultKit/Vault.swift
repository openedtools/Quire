// Notes, transcript sidecars, people, folders and the vault marker (sync-format.md sections 3 to
// 9), written byte for byte like the desktop's canonical writer.

import CryptoKit
import Foundation

public let formatLevel: Int64 = 1
public let maxNoteBytes = 2 * 1024 * 1024
public let maxSidecarBytes = 20 * 1024 * 1024
public let maxPersonBytes = 256 * 1024
public let maxFolderBytes = 16 * 1024

private let crockford = Array("0123456789ABCDEFGHJKMNPQRSTVWXYZ".unicodeScalars)

/// 26 Crockford base32 characters, the first at most 7 (48 bits of time come first).
public func isULID(_ text: String) -> Bool {
    let scalars = Array(text.unicodeScalars)
    return scalars.count == 26 && scalars.allSatisfy(crockford.contains) && scalars[0].value <= ("7" as Unicode.Scalar).value
}

public func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

// ===== JSON written like the desktop's serde_json =====

func jsonString(_ text: String) -> String {
    var out = "\""
    for scalar in text.unicodeScalars {
        switch scalar {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        case "\u{08}": out += "\\b"
        case "\u{0C}": out += "\\f"
        default:
            if scalar.value < 0x20 {
                out += String(format: "\\u%04x", scalar.value)
            } else {
                out.unicodeScalars.append(scalar)
            }
        }
    }
    return out + "\""
}

/// Shortest round-trip form with a `.0` on whole numbers, as serde_json writes an f64 for the
/// times a recording has (0 to a few hours). Exponent forms differ from serde's and are matched
/// before the phone writes sidecars (I3).
func jsonNumber(_ value: Double) -> String {
    guard value.isFinite else { return "null" }
    let text = "\(value)"
    return text.contains(".") || text.contains("e") ? text : text + ".0"
}

// ===== Notes (section 6) =====

public struct Speaker: Equatable, Sendable {
    public var slot: Int64
    public var name: String
    public var person: String?
    public init(slot: Int64, name: String, person: String?) { self.slot = slot; self.name = name; self.person = person }
}

public struct TranscriptRef: Equatable, Sendable {
    public var file: String
    public var sha256: String
    public var segments: Int64
    public init(file: String, sha256: String, segments: Int64) { self.file = file; self.sha256 = sha256; self.segments = segments }
}

public struct SummaryMeta: Equatable, Sendable {
    public var status: String
    public var generatedAt: String?
    public var model: String?
    public init(status: String, generatedAt: String?, model: String?) { self.status = status; self.generatedAt = generatedAt; self.model = model }
}

public struct AudioMeta: Equatable, Sendable {
    public var retention: String
    public var expires: String?
    public var state: String
    public var deletedAt: String?
    public init(retention: String, expires: String?, state: String, deletedAt: String?) {
        self.retention = retention; self.expires = expires; self.state = state; self.deletedAt = deletedAt
    }
}

public struct Note: Equatable, Sendable {
    public var id: String
    public var title: String
    public var created: String
    public var updated: String
    public var updatedBy: String
    public var rev: Int64
    public var started: String?
    public var ended: String?
    public var durationS: Int64?
    public var language: String?
    public var source: String
    public var mode: String
    public var speakers: [Speaker]
    public var transcript: TranscriptRef?
    public var summary: SummaryMeta?
    public var audio: AudioMeta
    public var extra: [(String, Value)]
    public var body: String

    public init(
        id: String, title: String, created: String, updated: String, updatedBy: String, rev: Int64, started: String?, ended: String?,
        durationS: Int64?, language: String?, source: String, mode: String, speakers: [Speaker], transcript: TranscriptRef?,
        summary: SummaryMeta?, audio: AudioMeta, extra: [(String, Value)], body: String
    ) {
        self.id = id; self.title = title; self.created = created; self.updated = updated; self.updatedBy = updatedBy; self.rev = rev
        self.started = started; self.ended = ended; self.durationS = durationS; self.language = language; self.source = source
        self.mode = mode; self.speakers = speakers; self.transcript = transcript; self.summary = summary; self.audio = audio
        self.extra = extra; self.body = body
    }

    public static func == (a: Note, b: Note) -> Bool {
        a.id == b.id && a.title == b.title && a.created == b.created && a.updated == b.updated && a.updatedBy == b.updatedBy && a.rev == b.rev
            && a.started == b.started && a.ended == b.ended && a.durationS == b.durationS && a.language == b.language && a.source == b.source
            && a.mode == b.mode && a.speakers == b.speakers && a.transcript == b.transcript && a.summary == b.summary && a.audio == b.audio
            && Value.map(a.extra) == Value.map(b.extra) && a.body == b.body
    }
}

private let noteKeys: Set<String> = [
    "quire", "id", "title", "created", "updated", "updated_by", "rev", "started", "ended", "duration_s", "language", "source", "mode", "speakers",
    "transcript", "summary", "audio",
]

private func optInt(_ n: Int64?) -> Value { n.map(Value.int) ?? .null }

public func renderNote(_ note: Note) -> String {
    let speakers = Value.list(note.speakers.map { s in
        .map([("slot", .int(s.slot)), ("name", .string(s.name)), ("person", .optionalString(s.person))])
    })
    let transcript = note.transcript.map { t in
        Value.map([("file", .string(t.file)), ("sha256", .string(t.sha256)), ("segments", .int(t.segments))])
    } ?? .null
    let summary = note.summary.map { s in
        Value.map([("status", .string(s.status)), ("generated_at", .optionalString(s.generatedAt)), ("model", .optionalString(s.model))])
    } ?? .null
    let audio = Value.map([
        ("retention", .string(note.audio.retention)),
        ("expires", .optionalString(note.audio.expires)),
        ("state", .string(note.audio.state)),
        ("deleted_at", .optionalString(note.audio.deletedAt)),
        ("file", .null),
        ("sha256", .null),
        ("bytes", .null),
    ])
    var entries: [(String, Value)] = [
        ("quire", .int(formatLevel)),
        ("id", .string(note.id)),
        ("title", .string(note.title)),
        ("created", .string(note.created)),
        ("updated", .string(note.updated)),
        ("updated_by", .string(note.updatedBy)),
        ("rev", .int(note.rev)),
        ("started", .optionalString(note.started)),
        ("ended", .optionalString(note.ended)),
        ("duration_s", optInt(note.durationS)),
        ("language", .optionalString(note.language)),
        ("source", .string(note.source)),
        ("mode", .string(note.mode)),
        ("speakers", speakers),
        ("transcript", transcript),
        ("summary", summary),
        ("audio", audio),
    ]
    entries += note.extra
    return writeDocument(entries, body: note.body)
}

private func req(_ map: Value, _ key: String) throws -> String {
    guard let s = map[key]?.stringValue else { throw FormatError("\(key) must be a string") }
    return s
}

private func opt(_ map: Value, _ key: String) throws -> String? {
    switch map[key] {
    case nil, .null?: return nil
    case let .str(s)?: return s
    default: throw FormatError("\(key) must be a string or null")
    }
}

public func parseNote(_ data: Data) throws -> Note {
    guard data.count <= maxNoteBytes else { throw FormatError("note file too large") }
    let (entries, body) = try parseDocument(data)
    let map = Value.map(entries)
    guard map["quire"]?.intValue == formatLevel else { throw FormatError("not a version 1 note") }
    let id = try req(map, "id")
    guard isULID(id) else { throw FormatError("id is not a ULID") }
    guard case let .list(items)? = map["speakers"] else { throw FormatError("speakers must be a list") }
    let speakers = try items.map { s in
        guard let slot = s["slot"]?.intValue else { throw FormatError("speaker slot") }
        return Speaker(slot: slot, name: try req(s, "name"), person: try opt(s, "person"))
    }
    var transcript: TranscriptRef?
    if let t = map["transcript"], t != .null {
        guard let segments = t["segments"]?.intValue else { throw FormatError("segments") }
        transcript = TranscriptRef(file: try req(t, "file"), sha256: try req(t, "sha256"), segments: segments)
    }
    var summary: SummaryMeta?
    if let s = map["summary"], s != .null {
        summary = SummaryMeta(status: try req(s, "status"), generatedAt: try opt(s, "generated_at"), model: try opt(s, "model"))
    }
    guard let a = map["audio"] else { throw FormatError("audio") }
    let duration: Int64?
    switch map["duration_s"] {
    case nil, .null?: duration = nil
    case let .int(n)?: duration = n
    default: throw FormatError("duration_s must be an integer or null")
    }
    guard let rev = map["rev"]?.intValue else { throw FormatError("rev") }
    return Note(
        id: id, title: try req(map, "title"), created: try req(map, "created"), updated: try req(map, "updated"),
        updatedBy: try req(map, "updated_by"), rev: rev, started: try opt(map, "started"), ended: try opt(map, "ended"),
        durationS: duration, language: try opt(map, "language"), source: try req(map, "source"), mode: try req(map, "mode"),
        speakers: speakers, transcript: transcript, summary: summary,
        audio: AudioMeta(retention: try req(a, "retention"), expires: try opt(a, "expires"), state: try req(a, "state"), deletedAt: try opt(a, "deleted_at")),
        extra: entries.filter { !noteKeys.contains($0.0) }, body: body)
}

// ===== Transcript sidecars (section 7) =====

public struct Segment: Equatable, Sendable {
    public var id: String
    public var start: Double
    public var end: Double
    public var speaker: Int64?
    public var text: String
    public init(id: String, start: Double, end: Double, speaker: Int64?, text: String) {
        self.id = id; self.start = start; self.end = end; self.speaker = speaker; self.text = text
    }
}

public func renderSidecar(noteID: String, segments: [Segment]) -> String {
    let sorted = segments.sorted { ($0.start, $0.id) < ($1.start, $1.id) }
    var out = "{\n  \"quire\": \(formatLevel),\n  \"note_id\": \(jsonString(noteID)),\n  \"segments\": ["
    for (i, s) in sorted.enumerated() {
        out += i == 0 ? "\n    " : ",\n    "
        out += "{\"id\":\(jsonString(s.id)),\"start\":\(jsonNumber(s.start)),\"end\":\(jsonNumber(s.end)),\"speaker\":\(s.speaker.map(String.init) ?? "null"),\"text\":\(jsonString(nfc(s.text)))}"
    }
    return out + (sorted.isEmpty ? "]\n}\n" : "\n  ]\n}\n")
}

public func sidecarName(noteID: String, sha256: String) -> String { "\(noteID).\(sha256.prefix(8)).transcript.json" }

/// Strict JSON as the desktop reads it: UTF-8 only, exact types (a boolean or 1.9 is not an
/// integer), and a duplicate key anywhere is an error.
private func decodeStrict<T: Decodable>(_ type: T.Type, _ data: Data) throws -> T {
    guard decodeUTF8(data) != nil, data.first == UInt8(ascii: "{") else { throw FormatError("not a UTF-8 JSON object") }
    guard !hasDuplicateKeys(data) else { throw FormatError("duplicate key") }
    do {
        return try JSONDecoder().decode(T.self, from: data)
    } catch {
        throw FormatError("JSON does not match the format")
    }
}

/// Looks for a repeated key in any object, in one pass over the raw JSON (JSONDecoder would
/// silently keep one of them).
func hasDuplicateKeys(_ data: Data) -> Bool {
    struct Frame {
        let isObject: Bool
        var expectingKey: Bool
        var keys: Set<[UInt8]> = []
    }
    var stack: [Frame] = []
    let bytes = [UInt8](data)
    var i = 0
    while i < bytes.count {
        switch bytes[i] {
        case UInt8(ascii: "{"): stack.append(Frame(isObject: true, expectingKey: true))
        case UInt8(ascii: "["): stack.append(Frame(isObject: false, expectingKey: false))
        case UInt8(ascii: "}"), UInt8(ascii: "]"): _ = stack.popLast()
        case UInt8(ascii: ","): if let top = stack.indices.last, stack[top].isObject { stack[top].expectingKey = true }
        case UInt8(ascii: ":"): if let top = stack.indices.last { stack[top].expectingKey = false }
        case UInt8(ascii: "\""):
            var j = i + 1
            while j < bytes.count, bytes[j] != UInt8(ascii: "\"") { j += bytes[j] == UInt8(ascii: "\\") ? 2 : 1 }
            if let top = stack.indices.last, stack[top].isObject, stack[top].expectingKey {
                let key = Array(bytes[(i + 1)..<min(j, bytes.count)])
                if !stack[top].keys.insert(key).inserted { return true }
            }
            i = j
        default: break
        }
        i += 1
    }
    return false
}

private struct SidecarFile: Decodable {
    let quire: Int64
    let note_id: String
    let segments: [SegmentFile]
}

private struct SegmentFile: Decodable {
    let id: String
    let start: Double
    let end: Double
    let speaker: Int64?
    let text: String
}

public func parseSidecar(_ data: Data) throws -> (String, [Segment]) {
    guard data.count <= maxSidecarBytes else { throw FormatError("sidecar too large") }
    let file = try decodeStrict(SidecarFile.self, data)
    guard file.quire == formatLevel, isULID(file.note_id) else { throw FormatError("not a version 1 sidecar") }
    return (file.note_id, file.segments.map { Segment(id: $0.id, start: $0.start, end: $0.end, speaker: $0.speaker, text: $0.text) })
}

/// A transcript hash as the format writes it: 64 lowercase hex characters (checked before any
/// path is built from it).
public func isSHA256(_ text: String) -> Bool {
    text.utf8.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
}

// ===== People (section 8) =====

public struct Person: Equatable, Sendable {
    public var id: String
    public var name: String
    public var aliases: [String]
    public var created: String
    public var updated: String
    public var updatedBy: String
    public var rev: Int64
    public var mergedInto: String?
    public var extra: [(String, Value)]
    public var body: String

    public static func == (a: Person, b: Person) -> Bool {
        a.id == b.id && a.name == b.name && a.aliases == b.aliases && a.created == b.created && a.updated == b.updated && a.updatedBy == b.updatedBy
            && a.rev == b.rev && a.mergedInto == b.mergedInto && Value.map(a.extra) == Value.map(b.extra) && a.body == b.body
    }
}

private let personKeys: Set<String> = ["quire", "id", "name", "aliases", "created", "updated", "updated_by", "rev", "merged_into"]

public func renderPerson(_ p: Person) -> String {
    var entries: [(String, Value)] = [
        ("quire", .int(formatLevel)), ("id", .string(p.id)), ("name", .string(p.name)), ("aliases", .list(p.aliases.map(Value.string))),
        ("created", .string(p.created)), ("updated", .string(p.updated)), ("updated_by", .string(p.updatedBy)), ("rev", .int(p.rev)),
        ("merged_into", .optionalString(p.mergedInto)),
    ]
    entries += p.extra
    return writeDocument(entries, body: p.body)
}

public func parsePerson(_ data: Data) throws -> Person {
    guard data.count <= maxPersonBytes else { throw FormatError("person file too large") }
    let (entries, body) = try parseDocument(data)
    let map = Value.map(entries)
    let id = try req(map, "id")
    guard isULID(id), map["quire"]?.intValue == formatLevel else { throw FormatError("not a version 1 person") }
    guard case let .list(items)? = map["aliases"] else { throw FormatError("aliases must be a list") }
    let aliases = try items.map { a -> String in
        guard let s = a.stringValue else { throw FormatError("alias") }
        return s
    }
    guard let rev = map["rev"]?.intValue else { throw FormatError("rev") }
    return Person(
        id: id, name: try req(map, "name"), aliases: aliases, created: try req(map, "created"), updated: try req(map, "updated"),
        updatedBy: try req(map, "updated_by"), rev: rev, mergedInto: try opt(map, "merged_into"),
        extra: entries.filter { !personKeys.contains($0.0) }, body: body)
}

// ===== Folders (section 9) and the marker (section 5) =====

public struct Folder: Equatable, Sendable {
    public var id: String
    public var name: String
    public var created: String
    public var updated: String
    public var updatedBy: String

    public init(id: String, name: String, created: String, updated: String, updatedBy: String) {
        self.id = id
        self.name = name
        self.created = created
        self.updated = updated
        self.updatedBy = updatedBy
    }
}

public func renderFolder(_ f: Folder) -> String {
    "{\"quire\":\(formatLevel),\"id\":\(jsonString(f.id)),\"name\":\(jsonString(nfc(f.name))),\"created\":\(jsonString(f.created)),\"updated\":\(jsonString(f.updated)),\"updated_by\":\(jsonString(f.updatedBy))}\n"
}

private struct FolderFile: Decodable {
    let quire: Int64
    let id: String
    let name: String
    let created: String
    let updated: String
    let updated_by: String
}

public func parseFolder(_ data: Data) throws -> Folder {
    guard data.count <= maxFolderBytes else { throw FormatError("folder file too large") }
    let f = try decodeStrict(FolderFile.self, data)
    guard f.quire == formatLevel, isULID(f.id) else { throw FormatError("not a version 1 folder") }
    return Folder(id: f.id, name: f.name, created: f.created, updated: f.updated, updatedBy: f.updated_by)
}

public struct Marker: Equatable, Sendable {
    public var format: String
    public var version: Int64
    public var minReader: Int64
    public var vaultID: String
    public var created: String
    public var createdBy: String

    public init(format: String, version: Int64, minReader: Int64, vaultID: String, created: String, createdBy: String) {
        self.format = format
        self.version = version
        self.minReader = minReader
        self.vaultID = vaultID
        self.created = created
        self.createdBy = createdBy
    }
}

public func renderMarker(_ m: Marker) -> String {
    """
    {
      "format": \(jsonString(m.format)),
      "version": \(m.version),
      "min_reader": \(m.minReader),
      "vault_id": \(jsonString(m.vaultID)),
      "created": \(jsonString(m.created)),
      "created_by": \(jsonString(m.createdBy))
    }

    """
}

private struct MarkerFile: Decodable {
    let format: String
    let version: Int64
    let min_reader: Int64
    let vault_id: String
    let created: String
    let created_by: String
}

public func parseMarker(_ data: Data) throws -> Marker {
    let m = try decodeStrict(MarkerFile.self, data)
    guard m.format == "quire-vault" else { throw FormatError("not a Quire vault") }
    return Marker(format: m.format, version: m.version, minReader: m.min_reader, vaultID: m.vault_id, created: m.created, createdBy: m.created_by)
}

// ===== Folder directory names (section 9) =====

private let reserved: Set<String> = Set(["CON", "PRN", "AUX", "NUL"] + (1...9).map { "COM\($0)" } + (1...9).map { "LPT\($0)" })

private func trimName(_ name: String) -> String {
    var scalars = Array(name.unicodeScalars)
    while let f = scalars.first, f == "." || f == "_" || f == " " { scalars.removeFirst() }
    while let l = scalars.last, l == "." || l == " " { scalars.removeLast() }
    return String(String.UnicodeScalarView(scalars))
}

/// Steps 1 to 5: the directory name for one folder, before collisions.
public func folderDirName(_ name: String) -> String {
    let kept = nfc(name).unicodeScalars.filter { !isControl($0) && !"<>:\"/\\|?*".unicodeScalars.contains($0) }
    var cleaned = trimName(String(String.UnicodeScalarView(kept)))
    var units = 0
    var cut = String.UnicodeScalarView()
    for scalar in cleaned.unicodeScalars {
        units += scalar.utf16.count
        if units > 80 { break }
        cut.append(scalar)
    }
    cleaned = trimName(String(cut))
    if cleaned.isEmpty { cleaned = "Folder" }
    // The word goes after the part before the first dot: `nul folder.txt` (Phase E amendment).
    // Split on the scalar, and upper-case ASCII only, as the desktop does.
    let scalars = Array(cleaned.unicodeScalars)
    let dot = scalars.firstIndex(of: ".")
    let base = String(String.UnicodeScalarView(scalars[..<(dot ?? scalars.count)]))
    let asciiUpper = String(String.UnicodeScalarView(base.unicodeScalars.map { ("a"..."z").contains($0) ? Unicode.Scalar($0.value - 32)! : $0 }))
    if reserved.contains(asciiUpper) {
        let rest = dot.map { String(String.UnicodeScalarView(scalars[($0 + 1)...])) }
        cleaned = rest.map { "\(base) folder.\($0)" } ?? "\(base) folder"
    }
    return cleaned
}

public func fold(_ name: String) -> String { nfc(name).lowercased() }

/// Step 6: when two cleaned names collide, the folder with the larger ID gets ` (2)`, ` (3)`.
public func folderDirs(_ folders: [(id: String, name: String)]) -> [(id: String, dir: String)] {
    var taken: Set<String> = []
    return folders.sorted { $0.id < $1.id }.map { f in
        let base = folderDirName(f.name)
        var candidate = base
        var n = 1
        while taken.contains(fold(candidate)) {
            n += 1
            candidate = "\(base) (\(n))"
        }
        taken.insert(fold(candidate))
        return (f.id, candidate)
    }
}
