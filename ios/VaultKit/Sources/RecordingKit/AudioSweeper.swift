// The retention sweep on the phone (spec F-IOS-PRIV-1, F-RET-4 to 7 and 13; sync-format.md
// section 10): deletes the phone's own audio and any vault copy once the note says it is expired
// or deleted, checks each file is gone, and never touches a file it cannot tie to a note. Runs at
// launch, on return to the foreground and in a background task; the clock is passed in.

import Foundation
import VaultKit

public struct SweepReport: Equatable, Sendable {
    /// Notes whose audio was deleted and checked gone
    public var deleted: [String] = []
    /// Files that could not be deleted; tried again next time
    public var failed = 0
    /// Due, but more than the guard allows without a yes: these were held back, nothing deleted.
    /// A yes covers exactly these; any that become due later are asked about again.
    public var heldBack: [String] = []
    /// The clock reads earlier than one already seen: nothing was deleted
    public var clockWentBack = false
    /// Audio files with no note, never deleted automatically
    public var unmanaged = 0

    public init() {}
}

public struct AudioSweeper: Sendable {
    /// The audio formats the vault names (section 10); nothing else is ever deleted there
    static let extensions: Set<String> = ["m4a", "mp4", "wav", "mp3", "flac", "ogg", "caf"]
    /// On the phone also the recording format (`.aac`, PI23), which never enters the vault
    static let localExtensions = extensions.union([RecordingAudio.fileExtension])

    public let localFolder: URL
    /// The vault folder when it can be reached now
    public let vaultRoot: URL?
    /// A vault is chosen, even if it cannot be reached right now (iCloud signed out, still starting)
    public let vaultChosen: Bool

    public init(localFolder: URL, vaultRoot: URL?, vaultChosen: Bool? = nil) {
        self.localFolder = localFolder
        self.vaultRoot = vaultRoot
        self.vaultChosen = vaultChosen ?? (vaultRoot != nil)
    }

    /// Audio files named `<ULID>.<ext>` directly in `dir`, by note ID.
    private func audioFiles(in dir: URL, extensions: Set<String> = Self.extensions) -> [(id: String, url: URL)] {
        ((try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []).compactMap { name in
            let url = dir.appendingPathComponent(name)
            let id = url.deletingPathExtension().lastPathComponent
            guard extensions.contains(url.pathExtension), isULID(id) else { return nil }
            return (id, url)
        }
    }

    /// Removes a plain file (never a link) and checks it is gone.
    private func remove(_ url: URL) -> Bool {
        var info = stat()
        guard lstat(url.path, &info) == 0 else { return true }
        guard (info.st_mode & S_IFMT) == S_IFREG else { return false }
        try? FileManager.default.removeItem(at: url)
        return lstat(url.path, &info) != 0
    }

    /// `writtenToVault`: notes the phone has written to the vault and read back (its bases).
    /// `confirmed`: the notes the person said yes to deleting after the guard held them back.
    public func sweep(
        localNotes: [Note], vaultNotes: [Note], writtenToVault: Set<String> = [], now: Date, latestSeen: Date?, confirmed: Set<String> = []
    ) -> SweepReport {
        var report = SweepReport()
        // While the phone still has its own copy, that copy governs: it has not moved in yet, and
        // anything in the vault under its ID is older or not the phone's
        var notes: [String: Note] = [:]
        for note in vaultNotes { notes[note.id] = note }
        for note in localNotes { notes[note.id] = note }
        let stillLocal = Set(localNotes.map(\.id))

        var files = audioFiles(in: localFolder, extensions: Self.localExtensions).map { (id: $0.id, url: $0.url, local: true) }
        if let vaultRoot {
            files += audioFiles(in: vaultRoot.appendingPathComponent("_audio")).map { (id: $0.id, url: $0.url, local: false) }
        }

        var withAudio = Set<String>()
        var due: [(id: String, url: URL)] = []
        for file in files {
            guard let note = notes[file.id] else {
                report.unmanaged += 1
                continue
            }
            withAudio.insert(file.id)
            switch audioStatus(note.audio, ended: note.ended, now: now) {
            case .expired, .deleted:
                // Off deletes once the transcript is confirmed in the vault, when there is one (PI15)
                let waitingForVault = file.local && vaultChosen && (stillLocal.contains(file.id) || !writtenToVault.contains(file.id))
                    && note.audio.state == "present" && note.audio.retention == Retention.off.rawValue
                if !waitingForVault { due.append((file.id, file.url)) }
            case .none, .kept:
                continue
            }
        }

        if clockWentBack(now: now, latestSeen: latestSeen) {
            report.clockWentBack = true
            return report
        }
        var dueIDs = Set(due.map(\.id))
        // A yes covers what it was asked about; the guard still applies to the rest
        let unasked = dueIDs.subtracting(confirmed)
        if needsConfirmation(due: unasked.count, withAudio: withAudio.count) {
            report.heldBack = unasked.sorted()
            dueIDs = dueIDs.intersection(confirmed)
            due = due.filter { dueIDs.contains($0.id) }
        }
        var failedIDs = Set<String>()
        for file in due where !remove(file.url) {
            report.failed += 1
            failedIDs.insert(file.id)
        }
        report.deleted = dueIDs.subtracting(failedIDs).sorted()
        return report
    }
}
