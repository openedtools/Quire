// What runs at launch, on return to the foreground, after a recording and in a background task
// (plan I3, PI13 to PI17): a recording cut short gets its deadline, finished recordings move into
// the vault, and the retention sweep deletes audio that is due. Recordings and imports in progress
// are never touched, and runs and edits happen one at a time.

import BackgroundTasks
import Foundation
import Observation
import RecordingKit
import Synchronization
import VaultKit

private let retentionKey = "audioRetention"
private let latestClockKey = "retentionLatestClock"
private let pendingFoldersKey = "pendingVaultFolders"

/// The retention for new recordings (Settings), the desktop's default until changed.
func currentRetention() -> Retention {
    UserDefaults.standard.string(forKey: retentionKey).flatMap(Retention.init) ?? .standard
}

/// What happens to a new recording's audio, in one sentence.
func retentionSentence(_ retention: Retention) -> String {
    switch retention {
    case .off: "Its audio is deleted as soon as the transcript is safely in the vault."
    case .keep: "Its audio is kept on this iPhone until you delete it."
    default: "Its audio stays on this iPhone and is deleted \(retention.label) after the recording ends."
    }
}

/// IDs of recordings and imports still being written; housekeeping leaves them alone.
enum InProgress {
    private static let ids = Mutex<Set<String>>([])
    static func add(_ id: String) { ids.withLock { _ = $0.insert(id) } }
    static func remove(_ id: String) { ids.withLock { _ = $0.remove(id) } }
    static func contains(_ id: String) -> Bool { ids.withLock { $0.contains(id) } }
}

/// The vault folder chosen for a recording, kept until it has moved in (PI14).
struct PendingFolders: @unchecked Sendable {
    var defaults = UserDefaults.standard

    func set(_ id: String, _ folderID: String?) {
        var all = defaults.dictionary(forKey: pendingFoldersKey) as? [String: String] ?? [:]
        all[id] = folderID
        defaults.set(all, forKey: pendingFoldersKey)
    }

    func get(_ id: String) -> String? {
        (defaults.dictionary(forKey: pendingFoldersKey) as? [String: String])?[id]
    }
}

/// What the phone last wrote for each of its notes in the vault (never in the vault itself).
func baseStoreURL() -> URL {
    FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("vault-bases.json")
}

struct HousekeepingResult: Sendable {
    var report = SweepReport()
    var moved = 0
    var problem: String?
    var phoneIDs: Set<String> = []
}

private func moveProblem(_ error: Error) -> String? {
    switch error as? VaultWriteError {
    case .changedDuringMove?: nil
    case .newerFormat?: "The vault was made by a newer version of Quire, so recordings stay on this iPhone. Update the app to move them."
    case .notAVault?: "The vault folder cannot be reached right now, so recordings stay on this iPhone and move later."
    case .nameTaken?: "A recording could not move to the vault because a different file there has its name. It stays on this iPhone."
    default: "A recording could not move to the vault yet. It stays on this iPhone and is tried again."
    }
}

/// The file work, away from the main thread. `vaultChosen` is true whenever a vault folder is
/// set, even if `vaultRoot` cannot be reached right now, so Off audio waits for it.
func housekeep(
    local: URL, vaultRoot: URL?, vaultChosen: Bool, now: Date, latestSeen: Date?, confirmed: Set<String>, deviceID: String,
    basesFile: URL = baseStoreURL(), folders: PendingFolders = PendingFolders()
) -> HousekeepingResult {
    var result = HousekeepingResult()
    func localNotes() -> [Note] {
        ((try? VaultReader(root: local).read().meetings) ?? []).map(\.note).filter { !InProgress.contains($0.id) }
    }

    // A recording the app was ended in the middle of gets its deadline now (F-RET-2), but never
    // from a clock that went back
    if !clockWentBack(now: now, latestSeen: latestSeen) {
        for var note in localNotes() where !isFinalized(note) {
            finalizeRecovered(&note, now: now, deviceID: deviceID)
            try? Data(renderNote(note).utf8).write(to: local.appendingPathComponent("\(note.id).md"), options: .atomic)
        }
    }

    var bases = BaseStore(file: basesFile)
    var vaultNotes: [Note] = []
    if let vaultRoot {
        let writer = VaultWriter(root: vaultRoot)
        for note in localNotes() {
            let url = local.appendingPathComponent("\(note.id).md")
            do {
                do {
                    _ = try writer.moveIn(note: url, folderID: folders.get(note.id)) { try bases.set($0) }
                } catch VaultWriteError.noSuchFolder {
                    // The folder was removed on the desktop: unfiled instead
                    _ = try writer.moveIn(note: url, folderID: nil) { try bases.set($0) }
                }
                folders.set(note.id, nil)
                result.moved += 1
            } catch {
                result.problem = result.problem ?? moveProblem(error)
            }
        }
        vaultNotes = ((try? VaultReader(root: vaultRoot).read().meetings) ?? []).map(\.note)
    }

    let remaining = localNotes()
    result.report = AudioSweeper(localFolder: local, vaultRoot: vaultRoot, vaultChosen: vaultChosen).sweep(
        localNotes: remaining, vaultNotes: vaultNotes, writtenToVault: Set(bases.records.keys), now: now, latestSeen: latestSeen, confirmed: confirmed)
    result.phoneIDs = Set(bases.records.keys).union(remaining.map(\.id)).union(vaultNotes.filter { $0.updatedBy == deviceID }.map(\.id))
    return result
}

@MainActor
@Observable
final class Housekeeper {
    static let taskID = "com.openedtools.notes.sweep"

    /// Recordings the guard held back; a yes deletes exactly these
    private(set) var heldBack: [String] = []
    var notice: String?
    /// Meetings this iPhone recorded: local, in the vault with a base, or last written by this iPhone
    private(set) var phoneIDs: Set<String>
    /// Runs and edits happen one after another, never at the same time
    private var tail: Task<Void, Never>?

    init() {
        phoneIDs = Set(BaseStore(file: baseStoreURL()).records.keys)
    }

    func isPhoneMeeting(_ note: Note) -> Bool {
        phoneIDs.contains(note.id) || note.updatedBy == phoneDeviceID()
    }

    private func serially(_ work: @escaping @MainActor () async -> Void) async {
        let previous = tail
        let task = Task { @MainActor in
            await previous?.value
            await work()
        }
        tail = task
        await task.value
    }

    func run(vault: VaultStore, confirmed: Set<String> = []) async {
        await serially { await self.runNow(vault: vault, confirmed: confirmed) }
    }

    private func runNow(vault: VaultStore, confirmed: Set<String>) async {
        let defaults = UserDefaults.standard
        let stored = defaults.double(forKey: latestClockKey)
        let latest = stored > 0 ? Date(timeIntervalSince1970: stored) : nil
        let now = Date()
        let local = localVaultFolder()
        let root = vault.rootURL
        let chosen = vault.isChosen
        let device = phoneDeviceID()
        let result = await Task.detached {
            housekeep(local: local, vaultRoot: root, vaultChosen: chosen, now: now, latestSeen: latest, confirmed: confirmed, deviceID: device)
        }.value
        defaults.set(max(stored, now.timeIntervalSince1970), forKey: latestClockKey)
        heldBack = result.report.heldBack
        phoneIDs = result.phoneIDs
        if result.report.clockWentBack {
            notice = "Audio deletion is paused: this iPhone's clock reads earlier than the last time Quire ran. Check the date and time."
        } else {
            notice = result.problem
        }
        vault.reloadLocal()
        if result.moved > 0 || !result.report.deleted.isEmpty { vault.reload() }
    }

    /// "Keep this one" or "Delete audio now" on one of the phone's meetings, wherever it is now.
    func change(_ id: String, vault: VaultStore, _ edit: @escaping @Sendable (inout Note) -> Void) async {
        await serially {
            let local = localVaultFolder().appendingPathComponent("\(id).md")
            let root = vault.rootURL
            let device = phoneDeviceID()
            let problem: String? = await Task.detached {
                if let data = try? Data(contentsOf: local), var note = try? parseNote(data) {
                    edit(&note)
                    return (try? Data(renderNote(note).utf8).write(to: local, options: .atomic)) == nil ? "The change could not be saved." : nil
                }
                guard let root else { return "The vault folder cannot be reached right now. Try again in a moment." }
                var bases = BaseStore(file: baseStoreURL())
                let writer = VaultWriter(root: root)
                guard let base = bases.records[id] ?? writer.recoverBase(id, deviceID: device) else { return "This meeting is read-only on this iPhone." }
                do {
                    do {
                        try bases.set(try writer.update(base, change: edit))
                    } catch VaultWriteError.missing {
                        // The file moved since (a folder renamed elsewhere): find it again
                        guard let found = writer.recoverBase(id, deviceID: device) else { throw VaultWriteError.missing }
                        try bases.set(try writer.update(found, change: edit))
                    }
                    return nil
                } catch VaultWriteError.changedElsewhere {
                    return "This meeting was changed on another device, so it was not changed here."
                } catch {
                    return "The vault could not be written. Try again in a moment."
                }
            }.value
            await self.runNow(vault: vault, confirmed: [])
            if let problem { self.notice = problem }
            vault.reload()
        }
    }

    /// Folders on the phone (plan PN2): made and renamed in the vault's folder format.
    /// Returns what `work` made, or nil when it failed (the notice says why).
    @discardableResult
    func folderWork<T: Sendable>(vault: VaultStore, _ work: @escaping @Sendable (VaultWriter) throws -> T) async -> T? {
        var made: T?
        await serially {
            guard let root = vault.rootURL else {
                self.notice = "Choose the vault folder in Settings first."
                return
            }
            let result: Result<T, Error> = await Task.detached { Result { try work(VaultWriter(root: root)) } }.value
            switch result {
            case let .success(value): made = value
            case let .failure(error):
                switch error as? VaultWriteError {
                case .emptyName?: self.notice = "A folder needs a name."
                case .notThisPhonesFolder?: self.notice = "This folder comes from the desktop, so it is renamed there."
                default: self.notice = "The folder could not be saved. Try again in a moment."
                }
            }
            vault.reload()
        }
        return made
    }

    /// Moves one of the phone's meetings into a folder (nil: no folder). One still on the phone
    /// moves there when it goes into the vault.
    func move(_ id: String, toFolder folderID: String?, vault: VaultStore) async {
        await serially {
            let local = localVaultFolder().appendingPathComponent("\(id).md")
            if FileManager.default.fileExists(atPath: local.path) {
                PendingFolders().set(id, folderID)
                return
            }
            guard let root = vault.rootURL else { return }
            let device = phoneDeviceID()
            let problem: String? = await Task.detached {
                var bases = BaseStore(file: baseStoreURL())
                let writer = VaultWriter(root: root)
                guard let base = bases.records[id] ?? writer.recoverBase(id, deviceID: device) else { return "This meeting is read-only on this iPhone." }
                do {
                    do {
                        try bases.set(try writer.moveNote(base, toFolder: folderID))
                    } catch VaultWriteError.missing {
                        // The file moved since (a folder renamed elsewhere): find it again
                        guard let found = writer.recoverBase(id, deviceID: device) else { throw VaultWriteError.missing }
                        try bases.set(try writer.moveNote(found, toFolder: folderID))
                    }
                    return nil
                } catch VaultWriteError.changedElsewhere {
                    return "This meeting was changed on another device, so it was not moved here."
                } catch {
                    return "The meeting could not be moved. Try again in a moment."
                }
            }.value
            if let problem { self.notice = problem }
            vault.reload()
        }
    }

    func dismissNotice() { notice = nil }

    /// "Not now" on the guard: asked again at the next run
    func postpone() { heldBack = [] }

    /// Asks iOS for a background run; iOS decides when (PI17).
    static func scheduleBackground() {
        let request = BGAppRefreshTaskRequest(identifier: taskID)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 6 * 3600)
        try? BGTaskScheduler.shared.submit(request)
    }
}
