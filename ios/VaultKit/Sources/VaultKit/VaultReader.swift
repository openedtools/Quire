// Reads a whole vault into memory for the app, treating every file as untrusted input
// (sync-format.md section 14): names are checked against IDs, every path is derived from
// validated IDs and stays inside the vault, links are not followed, size limits hold, and a file
// that does not parse is skipped and reported, never repaired or removed. Read-only.

import Foundation

public struct VaultMeeting: Identifiable, Equatable, Sendable {
    public var id: String { note.id }
    public var note: Note
    /// The folder's ID, or nil at the vault root
    public var folderID: String?
    /// nil while the transcript is still syncing (missing, or its hash does not match yet)
    public var segments: [Segment]?
}

public struct VaultContents: Equatable, Sendable {
    public var marker: Marker?
    public var folders: [Folder] = []
    public var meetings: [VaultMeeting] = []
    public var people: [Person] = []
    /// Files skipped because they are not valid, counted by kind (never their names or content)
    public var skipped: [String: Int] = [:]
    /// Files iCloud has not downloaded yet; asked for, read on a later pass
    public var waitingForICloud = 0
    /// The vault was made by a newer app (its `min_reader` is above this app's level): shown,
    /// never written (section 13)
    public var newerFormat = false

    public func person(_ id: String?) -> Person? { id.flatMap { pid in people.first { $0.id == pid } } }

    public init() {}
}

public enum VaultReadError: Error, Equatable {
    case notAVault
    /// The marker is there but cannot be read yet (half written, or still downloading)
    case notReady
}

/// The longest recording the app accepts as real, in seconds (a day); larger or non-finite
/// numbers from a vault file are treated as invalid, never shown or computed with.
public let maxSeconds: Double = 86_400

private func validSegments(_ segments: [Segment]) -> Bool {
    segments.allSatisfy { $0.start.isFinite && $0.end.isFinite && $0.start >= 0 && $0.end >= 0 && $0.start <= maxSeconds && $0.end <= maxSeconds }
}

private let reservedDirs: Set<String> = ["_people", "_audio", "_trash", "_conflicts", "_tmp"]

public struct VaultReader {
    let root: URL
    let fm = FileManager.default

    public init(root: URL) { self.root = root.standardizedFileURL }

    private func isPlainFile(_ url: URL) -> Bool {
        let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        return values?.isRegularFile == true && values?.isSymbolicLink != true
    }

    private func isPlainDir(_ url: URL) -> Bool {
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        return values?.isDirectory == true && values?.isSymbolicLink != true
    }

    /// True for an iCloud file whose bytes are not on this phone yet; asks for them.
    private func needsDownload(_ url: URL) -> Bool {
        let values = try? url.resourceValues(forKeys: [.isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey])
        guard values?.isUbiquitousItem == true, let status = values?.ubiquitousItemDownloadingStatus, status != .current else { return false }
        try? fm.startDownloadingUbiquitousItem(at: url)
        return true
    }

    /// Reads a file of at most `limit` bytes, or nil (missing, a link, too large). The file is
    /// opened without following links and read with a bound, so nothing can change in between.
    private func read(_ url: URL, limit: Int) -> Data? {
        guard url.standardizedFileURL.path.hasPrefix(root.path + "/") else { return nil }
        let fd = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var info = stat()
        guard fstat(fd, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG, Int(info.st_size) <= limit else { return nil }
        var data = Data(count: Int(info.st_size) + 1)
        let n = data.withUnsafeMutableBytes { Darwin.read(fd, $0.baseAddress, $0.count) }
        guard n >= 0, n <= limit else { return nil }
        return data.prefix(n)
    }

    /// An iCloud placeholder (`.name.icloud`) for `name` in `dir`: asks for the download.
    private func requestDownload(_ dir: URL, _ name: String) -> Bool {
        let placeholder = dir.appendingPathComponent(".\(name).icloud")
        guard fm.fileExists(atPath: placeholder.path) else { return false }
        try? fm.startDownloadingUbiquitousItem(at: dir.appendingPathComponent(name))
        return true
    }

    private func names(in dir: URL) -> [String] {
        ((try? fm.contentsOfDirectory(atPath: dir.path)) ?? []).sorted()
    }

    /// The real name inside an iCloud placeholder, `.X.icloud` -> `X`.
    private func logicalNames(in dir: URL) -> [String] {
        names(in: dir).map { n in
            n.hasPrefix(".") && n.hasSuffix(".icloud") ? String(n.dropFirst().dropLast(7)) : n
        }
    }

    public func read() throws -> VaultContents {
        var out = VaultContents()
        let markerURL = root.appendingPathComponent("quire-format.json")
        if needsDownload(markerURL) {
            throw VaultReadError.notReady
        } else if let data = read(markerURL, limit: maxFolderBytes) {
            guard let marker = try? parseMarker(data) else {
                // A Quire marker that does not parse yet is half written: try again later
                let text = decodeUTF8(data) ?? ""
                throw text.contains("\"quire-vault\"") || text.isEmpty ? VaultReadError.notReady : VaultReadError.notAVault
            }
            out.newerFormat = marker.minReader > formatLevel
            out.marker = marker
        } else if requestDownload(root, "quire-format.json") {
            out.waitingForICloud += 1
        } else {
            throw VaultReadError.notAVault
        }

        func skip(_ kind: String) { out.skipped[kind, default: 0] += 1 }

        // Notes at the root and in each folder directory (depth 1 in version 1)
        var noteDirs: [(URL, String?)] = [(root, nil)]
        for name in names(in: root) where !name.hasPrefix(".") && !name.hasPrefix("_") {
            let dir = root.appendingPathComponent(name)
            guard isPlainDir(dir) else { continue }
            let folderURL = dir.appendingPathComponent("_folder.json")
            if needsDownload(folderURL) {
                out.waitingForICloud += 1
            } else if let data = read(folderURL, limit: maxFolderBytes) {
                if let folder = try? parseFolder(data), !out.folders.contains(where: { $0.id == folder.id }) {
                    out.folders.append(folder)
                    noteDirs.append((dir, folder.id))
                } else {
                    skip("folder")
                }
            } else if requestDownload(dir, "_folder.json") {
                out.waitingForICloud += 1
            }
        }

        for (dir, folderID) in noteDirs {
            for name in logicalNames(in: dir) where name.hasSuffix(".md") {
                let stem = String(name.dropLast(3))
                guard isULID(stem) else { continue }
                let url = dir.appendingPathComponent(name)
                if needsDownload(url) {
                    out.waitingForICloud += 1
                    continue
                }
                guard let data = read(url, limit: maxNoteBytes) else {
                    if requestDownload(dir, name) { out.waitingForICloud += 1 } else { skip("note") }
                    continue
                }
                guard var note = try? parseNote(data), note.id == stem else {
                    skip("note")
                    continue
                }
                // One note per ID: a second copy (an iCloud duplicate, or a move in progress) waits for Phase F
                if out.meetings.contains(where: { $0.id == note.id }) { continue }
                if let d = note.durationS, d < 0 || Double(d) > maxSeconds { note.durationS = nil }
                var segments: [Segment]?
                if let t = note.transcript {
                    // The sidecar's name is derived from the note's ID and a checked hash, never taken as a path
                    let file = isSHA256(t.sha256) ? sidecarName(noteID: note.id, sha256: t.sha256) : ""
                    let sideURL = dir.appendingPathComponent(file)
                    if file.isEmpty || file != t.file {
                        skip("transcript")
                    } else if needsDownload(sideURL) || requestDownload(dir, file) {
                        out.waitingForICloud += 1
                    } else if let side = read(sideURL, limit: maxSidecarBytes), sha256Hex(side) == t.sha256,
                              let (owner, segs) = try? parseSidecar(side), owner == note.id, validSegments(segs) {
                        var seen = Set<String>()
                        segments = segs.filter { seen.insert($0.id).inserted }
                    } else {
                        skip("transcript")
                    }
                } else {
                    segments = []
                }
                out.meetings.append(VaultMeeting(note: note, folderID: folderID, segments: segments))
            }
        }

        let peopleDir = root.appendingPathComponent("_people")
        if isPlainDir(peopleDir) {
            for name in logicalNames(in: peopleDir) where name.hasSuffix(".md") && isULID(String(name.dropLast(3))) {
                let url = peopleDir.appendingPathComponent(name)
                if needsDownload(url) {
                    out.waitingForICloud += 1
                    continue
                }
                guard let data = read(url, limit: maxPersonBytes) else {
                    if requestDownload(peopleDir, name) { out.waitingForICloud += 1 } else { skip("person") }
                    continue
                }
                if let person = try? parsePerson(data), person.id == String(name.dropLast(3)), !out.people.contains(where: { $0.id == person.id }) {
                    out.people.append(person)
                } else {
                    skip("person")
                }
            }
        }

        out.meetings.sort { ($0.note.created, $0.note.id) > ($1.note.created, $1.note.id) }
        out.folders.sort { byName($0.name, $0.id, $1.name, $1.id) }
        out.people.sort { byName($0.name, $0.id, $1.name, $1.id) }
        return out
    }
}

/// Names in the order a person expects, then by ID so equal names always sort the same way.
private func byName(_ a: String, _ aID: String, _ b: String, _ bID: String) -> Bool {
    switch a.localizedStandardCompare(b) {
    case .orderedAscending: return true
    case .orderedDescending: return false
    case .orderedSame: return aID < bID
    }
}

/// The parts of a note's body: the generated summary and the person's own notes. Text under
/// no heading (a typed or imported note) counts as notes.
public func bodySections(_ body: String) -> (summary: String?, notes: String?) {
    var summary: [String] = []
    var notes: [String] = []
    var inSummary = false
    for raw in body.unicodeScalars.split(separator: "\n", omittingEmptySubsequences: false) {
        var scalars = Array(raw)
        if scalars.last == "\r" { scalars.removeLast() }
        let line = String(String.UnicodeScalarView(scalars))
        if line == "## Summary" { inSummary = true; continue }
        if line == "## Notes" { inSummary = false; continue }
        if inSummary { summary.append(line) } else { notes.append(line) }
    }
    func clean(_ lines: [String]) -> String? {
        let text = lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }
    return (clean(summary), clean(notes))
}
