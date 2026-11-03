// Writes the phone's own meetings into the vault (spec F-IOS-VAULT-3, plan PI13 and PI14):
// sidecar first, then the note (sync-format.md section 15), each through a file coordinator so
// iCloud sees whole files; both read back and compared before the copy in the app's own folder is
// removed. Audio never goes in (F-IOS-VAULT-5). A later change is written only over the copy the
// phone last wrote; anything else is left alone (section 11, the phone's base).

import Foundation
import VaultKit

public enum VaultWriteError: Error, Equatable {
    /// No Quire marker in the folder
    case notAVault
    /// Made by a newer app: read, never written (section 13)
    case newerFormat
    /// The chosen folder is not in the vault (any more)
    case noSuchFolder
    /// A different file already has that name; it is never overwritten
    case nameTaken
    /// The local transcript is missing or does not match the note
    case sidecarMissing
    /// What was written did not read back the same
    case readBackFailed
    /// Another device changed the note since the phone wrote it
    case changedElsewhere
    /// A folder needs a name
    case emptyName
    /// Only folders this phone made can be renamed here (the desktop owns its own)
    case notThisPhonesFolder
    /// The note is no longer where the phone wrote it
    case missing
    /// The local copy changed while it was being moved; it stays and moves next time
    case changedDuringMove
}

/// What the phone last wrote for one of its notes: where, its hash and its rev (the base).
public struct BaseRecord: Codable, Equatable, Sendable {
    public let id: String
    /// Relative to the vault root, with forward slashes
    public let path: String
    public let sha256: String
    public let rev: Int64

    public init(id: String, path: String, sha256: String, rev: Int64) {
        self.id = id
        self.path = path
        self.sha256 = sha256
        self.rev = rev
    }
}

/// The bases, kept in the app's own storage (never in the vault).
public struct BaseStore: Sendable {
    public let file: URL
    public private(set) var records: [String: BaseRecord] = [:]

    public init(file: URL) {
        self.file = file
        if let data = try? Data(contentsOf: file), let list = try? JSONDecoder().decode([BaseRecord].self, from: data) {
            records = Dictionary(list.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        }
    }

    public mutating func set(_ record: BaseRecord) throws {
        records[record.id] = record
        try save()
    }

    /// After a folder's directory is renamed: notes that were in it are now under `to`.
    public mutating func rebase(dir from: String, to: String) throws {
        guard from != to else { return }
        let prefix = from + "/"
        var changed = false
        for (id, r) in records where r.path.hasPrefix(prefix) {
            records[id] = BaseRecord(id: r.id, path: to + "/" + r.path.dropFirst(prefix.count), sha256: r.sha256, rev: r.rev)
            changed = true
        }
        if changed { try save() }
    }

    public mutating func remove(_ id: String) throws {
        records[id] = nil
        try save()
    }

    private func save() throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        let list = records.values.sorted { $0.id < $1.id }
        try JSONEncoder().encode(list).write(to: file, options: .atomic)
    }
}

public struct VaultWriter: Sendable {
    public let root: URL

    public init(root: URL) { self.root = root.standardizedFileURL }

    /// Moves a finished meeting from the app's own folder into the vault, at the root or in an
    /// existing folder. Safe to repeat: a move cut short is finished the next time.
    /// `record` keeps the base before the local copy is removed, so the phone never loses track of
    /// a note it wrote.
    public func moveIn(note localNote: URL, folderID: String?, record: (BaseRecord) throws -> Void = { _ in }) throws -> BaseRecord {
        try checkMarker()
        let noteData = try Data(contentsOf: localNote)
        let note = try parseNote(noteData)
        let localDir = localNote.deletingLastPathComponent()

        var sidecar: (name: String, data: Data)?
        if let ref = note.transcript {
            guard let data = try? Data(contentsOf: localDir.appendingPathComponent(ref.file)), sha256Hex(data) == ref.sha256 else {
                throw VaultWriteError.sidecarMissing
            }
            sidecar = (ref.file, data)
        }

        let dirName = try folderID.map(directory(of:))
        let dir = dirName.map { root.appendingPathComponent($0) } ?? root
        let noteName = "\(note.id).md"
        let noteURL = dir.appendingPathComponent(noteName)

        // Never write over someone else's file. The same bytes mean an earlier move got this far; an
        // older copy the phone itself wrote (a move cut short before a later local change) is replaced
        if let existing = try? coordinatedRead(noteURL), existing != noteData {
            guard let theirs = try? parseNote(existing), theirs.id == note.id, theirs.updatedBy == note.updatedBy, theirs.rev <= note.rev else {
                throw VaultWriteError.nameTaken
            }
        }
        if let sidecar {
            let url = dir.appendingPathComponent(sidecar.name)
            if let existing = try? coordinatedRead(url) {
                guard existing == sidecar.data else { throw VaultWriteError.nameTaken }
            } else {
                try coordinatedWrite(sidecar.data, to: url)
            }
        }
        try coordinatedWrite(noteData, to: noteURL)

        // Read back before anything local is removed
        guard (try? coordinatedRead(noteURL)) == noteData else { throw VaultWriteError.readBackFailed }
        if let sidecar {
            guard (try? coordinatedRead(dir.appendingPathComponent(sidecar.name))) == sidecar.data else { throw VaultWriteError.readBackFailed }
        }
        let path = dirName.map { "\($0)/\(noteName)" } ?? noteName
        let base = BaseRecord(id: note.id, path: path, sha256: sha256Hex(noteData), rev: note.rev)
        try record(base)
        // A change made on the phone while this ran stays local and moves next time
        guard (try? Data(contentsOf: localNote)) == noteData else { throw VaultWriteError.changedDuringMove }
        if let sidecar { try? FileManager.default.removeItem(at: localDir.appendingPathComponent(sidecar.name)) }
        try FileManager.default.removeItem(at: localNote)
        return base
    }

    /// Changes one of the phone's notes in the vault ("Keep this one", "Delete audio now"), only
    /// if the file is still exactly what the phone last wrote. Returns the new base.
    public func update(_ base: BaseRecord, change: (inout Note) -> Void) throws -> BaseRecord {
        try checkMarker()
        let url = root.appendingPathComponent(base.path)
        guard let data = try? coordinatedRead(url) else { throw VaultWriteError.missing }
        guard sha256Hex(data) == base.sha256 else { throw VaultWriteError.changedElsewhere }
        var note = try parseNote(data)
        let before = note
        change(&note)
        guard note != before else { return base }
        let out = Data(renderNote(note).utf8)
        try coordinatedWrite(out, to: url)
        guard (try? coordinatedRead(url)) == out else { throw VaultWriteError.readBackFailed }
        return BaseRecord(id: note.id, path: base.path, sha256: sha256Hex(out), rev: note.rev)
    }

    /// Makes a folder at the top of the vault (sync-format section 9, plan PN2). Its directory
    /// is named from the name, with " (2)" and so on when another folder already has it (the new
    /// folder always has the larger ID).
    public func createFolder(named name: String, deviceID: String, now: Date) throws -> Folder {
        try checkMarker()
        let clean = nfc(name.trimmingCharacters(in: .whitespacesAndNewlines))
        guard !clean.isEmpty else { throw VaultWriteError.emptyName }
        let stamp = timestampString(now)
        let folder = Folder(id: newULID(at: now), name: clean, created: stamp, updated: stamp, updatedBy: deviceID)
        let dir = root.appendingPathComponent(freeDirName(for: clean, except: nil))
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: false)
        try writeFolder(folder, in: dir)
        return folder
    }

    /// Renames a folder this phone made: its directory and its `_folder.json`. Notes inside keep
    /// their bytes. Returns the folder and its directory before and after, for `BaseStore.rebase`.
    public func renameFolder(_ id: String, to name: String, deviceID: String, now: Date) throws -> (folder: Folder, from: String, to: String) {
        try checkMarker()
        let clean = nfc(name.trimmingCharacters(in: .whitespacesAndNewlines))
        guard !clean.isEmpty else { throw VaultWriteError.emptyName }
        let current = try directory(of: id)
        var folder = try parseFolder(try coordinatedRead(root.appendingPathComponent(current).appendingPathComponent("_folder.json")))
        guard folder.updatedBy == deviceID else { throw VaultWriteError.notThisPhonesFolder }
        folder.name = clean
        folder.updated = timestampString(now)
        var dir = root.appendingPathComponent(current)
        let wanted = freeDirName(for: clean, except: current)
        if wanted != current {
            let target = root.appendingPathComponent(wanted)
            if wanted.lowercased() == current.lowercased() {
                // Only the letter case changes: the same directory on a case-insensitive volume, so
                // through a temporary name (as the desktop does)
                let staging = root.appendingPathComponent("_tmp-\(id)")
                try coordinatedMove(dir, to: staging)
                try coordinatedMove(staging, to: target)
            } else {
                try coordinatedMove(dir, to: target)
            }
            dir = target
        }
        try writeFolder(folder, in: dir)
        return (folder, current, wanted)
    }

    /// Moves one of the phone's notes (and its transcript) into a folder or to the vault root,
    /// only if the note is still exactly what the phone last wrote. Returns the new base.
    public func moveNote(_ base: BaseRecord, toFolder folderID: String?) throws -> BaseRecord {
        try checkMarker()
        let from = root.appendingPathComponent(base.path)
        guard let data = try? coordinatedRead(from) else { throw VaultWriteError.missing }
        guard sha256Hex(data) == base.sha256 else { throw VaultWriteError.changedElsewhere }
        let note = try parseNote(data)
        let dirName = try folderID.map(directory(of:))
        let path = dirName.map { "\($0)/\(note.id).md" } ?? "\(note.id).md"
        guard path != base.path else { return base }
        let fromDir = from.deletingLastPathComponent()
        let toDir = dirName.map { root.appendingPathComponent($0) } ?? root
        var files = [(name: "\(note.id).md", data: data)]
        if let ref = note.transcript {
            // Never move a note away from its transcript
            guard let side = try? coordinatedRead(fromDir.appendingPathComponent(ref.file)), sha256Hex(side) == ref.sha256 else {
                throw VaultWriteError.sidecarMissing
            }
            files.insert((ref.file, side), at: 0)
        }
        // Sidecar first, then the note, each read back; never over a different file
        for file in files {
            let url = toDir.appendingPathComponent(file.name)
            if let existing = try? coordinatedRead(url) {
                guard existing == file.data else { throw VaultWriteError.nameTaken }
            } else {
                try coordinatedWrite(file.data, to: url)
            }
            guard (try? coordinatedRead(url)) == file.data else { throw VaultWriteError.readBackFailed }
        }
        // The note first: if it cannot go, the old base still points at a real file and a retry
        // finds the new copy already in place
        try coordinatedDelete(fromDir.appendingPathComponent(files[files.count - 1].name))
        if files.count > 1 { try? coordinatedDelete(fromDir.appendingPathComponent(files[0].name)) }
        return BaseRecord(id: note.id, path: path, sha256: base.sha256, rev: base.rev)
    }

    /// A directory name for `name` that no other folder uses, compared ignoring case.
    private func freeDirName(for name: String, except own: String?) -> String {
        let fold = { (s: String) in nfc(s).folding(options: [.caseInsensitive], locale: nil) }
        let taken = Set(((try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? []).filter { $0 != own }.map(fold))
        let base = folderDirName(name)
        var candidate = base
        var n = 2
        while taken.contains(fold(candidate)) {
            candidate = "\(base) (\(n))"
            n += 1
        }
        return candidate
    }

    private func writeFolder(_ folder: Folder, in dir: URL) throws {
        let out = Data(renderFolder(folder).utf8)
        let url = dir.appendingPathComponent("_folder.json")
        try coordinatedWrite(out, to: url)
        guard (try? coordinatedRead(url)) == out else { throw VaultWriteError.readBackFailed }
    }

    private func coordinatedMove(_ from: URL, to: URL) throws {
        var coordinationError: NSError?
        var moveError: Error?
        NSFileCoordinator(filePresenter: nil).coordinate(writingItemAt: from, options: .forMoving, writingItemAt: to, options: .forReplacing, error: &coordinationError) { a, b in
            do { try FileManager.default.moveItem(at: a, to: b) } catch { moveError = error }
        }
        if let error = coordinationError ?? moveError { throw error }
    }

    private func coordinatedDelete(_ url: URL) throws {
        var coordinationError: NSError?
        var deleteError: Error?
        NSFileCoordinator(filePresenter: nil).coordinate(writingItemAt: url, options: .forDeleting, error: &coordinationError) { target in
            do { try FileManager.default.removeItem(at: target) } catch { deleteError = error }
        }
        if let error = coordinationError ?? deleteError { throw error }
    }

    /// Where the vault holds note `id` (root or a folder), or nil.
    public func find(_ id: String) -> String? {
        guard isULID(id) else { return nil }
        let name = "\(id).md"
        if FileManager.default.fileExists(atPath: root.appendingPathComponent(name).path) { return name }
        for dir in (try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? [] where !dir.hasPrefix("_") && !dir.hasPrefix(".") {
            if FileManager.default.fileExists(atPath: root.appendingPathComponent(dir).appendingPathComponent(name).path) { return "\(dir)/\(name)" }
        }
        return nil
    }

    /// A base for a note the phone wrote but lost track of: only when the file still says the phone
    /// wrote it last, so nothing another device changed is ever taken over.
    public func recoverBase(_ id: String, deviceID: String) -> BaseRecord? {
        guard let path = find(id), let data = try? coordinatedRead(root.appendingPathComponent(path)),
              let note = try? parseNote(data), note.id == id, note.updatedBy == deviceID else { return nil }
        return BaseRecord(id: id, path: path, sha256: sha256Hex(data), rev: note.rev)
    }

    private func checkMarker() throws {
        guard let data = try? coordinatedRead(root.appendingPathComponent("quire-format.json")), let marker = try? parseMarker(data) else {
            throw VaultWriteError.notAVault
        }
        guard marker.minReader <= formatLevel else { throw VaultWriteError.newerFormat }
    }

    /// The directory of a folder, found by the ID in its `_folder.json` (the name is only for people)
    private func directory(of folderID: String) throws -> String {
        let fm = FileManager.default
        for name in (try? fm.contentsOfDirectory(atPath: root.path)) ?? [] where !name.hasPrefix("_") && !name.hasPrefix(".") {
            let dir = root.appendingPathComponent(name)
            let values = try? dir.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values?.isDirectory == true, values?.isSymbolicLink != true else { continue }
            if let data = try? Data(contentsOf: dir.appendingPathComponent("_folder.json")), (try? parseFolder(data))?.id == folderID {
                return name
            }
        }
        throw VaultWriteError.noSuchFolder
    }

    private func coordinatedWrite(_ data: Data, to url: URL) throws {
        var coordinationError: NSError?
        var writeError: Error?
        NSFileCoordinator(filePresenter: nil).coordinate(writingItemAt: url, options: .forReplacing, error: &coordinationError) { target in
            do { try data.write(to: target, options: .atomic) } catch { writeError = error }
        }
        if let error = coordinationError ?? writeError { throw error }
    }

    private func coordinatedRead(_ url: URL) throws -> Data {
        var coordinationError: NSError?
        var result: Result<Data, Error> = .failure(VaultWriteError.missing)
        NSFileCoordinator(filePresenter: nil).coordinate(readingItemAt: url, options: [], error: &coordinationError) { target in
            result = Result { try Data(contentsOf: target) }
        }
        if let coordinationError { throw coordinationError }
        return try result.get()
    }
}
