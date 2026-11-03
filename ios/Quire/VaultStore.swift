// The vault the app reads: the folder the person chose in iCloud Drive, kept with a bookmark
// (spec F-IOS-VAULT-1). Desktop meetings are shown, never changed; the phone adds its own (I3).

import Foundation
import Observation
import VaultKit

@MainActor
@Observable
final class VaultStore {
    enum State: Equatable {
        case none
        case loading
        case ready
        case failed(String)
    }

    private(set) var state: State = .none
    private(set) var contents = VaultContents()
    /// Meetings recorded on this iPhone, in the app's own folder until I3 moves them to the vault
    private(set) var local = VaultContents()
    private(set) var folderName: String?
    private var root: URL?
    /// The vault folder, for writing the phone's own meetings (the writer checks it is a vault)
    var rootURL: URL? { root }
    /// A vault folder is set, even when it cannot be reached right now (Off audio waits for it)
    var isChosen: Bool { root != nil || UserDefaults.standard.data(forKey: bookmarkKey) != nil }
    private var accessing = false
    /// Each read carries the generation it started in; a stale result is dropped
    private var generation = 0
    private var retries = 0
    private let bookmarkKey = "vaultBookmark"

    init() {
        // Tests and screenshots point at a generated vault; the app itself uses the bookmark
        if let path = ProcessInfo.processInfo.environment["QUIRE_VAULT_PATH"] {
            open(URL(fileURLWithPath: path), remember: false)
        } else if let data = UserDefaults.standard.data(forKey: bookmarkKey) {
            var stale = false
            if let url = try? URL(resolvingBookmarkData: data, bookmarkDataIsStale: &stale) {
                open(url, remember: stale)
            } else {
                state = .failed("The vault folder is no longer available. Choose it again in Settings.")
            }
        }
    }

    /// Opens a folder the person chose (after the disclosure) and remembers it.
    func open(_ url: URL, remember: Bool = true) {
        stopAccessing()
        accessing = url.startAccessingSecurityScopedResource()
        root = url
        retries = 0
        folderName = url.lastPathComponent
        if remember, let data = try? url.bookmarkData(options: .minimalBookmark) {
            UserDefaults.standard.set(data, forKey: bookmarkKey)
        }
        reload()
    }

    private func stopAccessing() {
        if accessing { root?.stopAccessingSecurityScopedResource() }
        accessing = false
    }

    func forget() {
        stopAccessing()
        generation += 1
        root = nil
        folderName = nil
        contents = VaultContents()
        state = .none
        UserDefaults.standard.removeObject(forKey: bookmarkKey)
    }

    func reload() {
        guard let root else { return }
        generation += 1
        let current = generation
        // A refresh keeps what is on screen; only a first read shows the spinner
        if state != .ready { state = .loading }
        Task.detached { [root] in
            let result = Result { try VaultReader(root: root).read() }
            await MainActor.run {
                guard current == self.generation else { return }
                switch result {
                case let .success(contents):
                    self.contents = contents
                    self.state = .ready
                    self.retryIfWaiting(contents.waitingForICloud > 0)
                case let .failure(error):
                    if error as? VaultReadError == .notReady, self.retries < 10 {
                        self.retryIfWaiting(true)
                    } else {
                        self.state = .failed(Self.message(error))
                    }
                }
            }
        }
    }

    /// Files iCloud is still downloading are read again, waiting longer each time, up to 10 tries.
    private func retryIfWaiting(_ waiting: Bool) {
        guard waiting, retries < 10 else {
            if !waiting { retries = 0 }
            return
        }
        retries += 1
        let delay = min(3 * (1 << (retries - 1)), 60)
        Task { try? await Task.sleep(for: .seconds(delay)); self.reload() }
    }

    private static func message(_ error: Error) -> String {
        switch error as? VaultReadError {
        case .notAVault?: return "This folder is not a Quire vault. Choose the Quire folder the desktop app writes in iCloud Drive."
        case .notReady?: return "The vault is still arriving from iCloud. Try again in a moment."
        default: return "The vault could not be read."
        }
    }

    func reloadLocal() {
        let folder = localVaultFolder()
        Task.detached {
            let result = (try? VaultReader(root: folder).read()) ?? VaultContents()
            await MainActor.run { self.local = result }
        }
    }

    func meetings(in folderID: String?) -> [VaultMeeting] {
        contents.meetings.filter { $0.folderID == folderID }
    }
}

/// The disclosure shown before a vault folder is used (the desktop's F-SYNC-2 wording).
let vaultDisclosure = """
Quire reads your meetings, notes, people and folders from this folder. It also writes the meetings you record on this iPhone there. \
If the folder is in iCloud Drive, Apple's iCloud uploads the files, not this app. Without Advanced Data Protection turned on in your Apple account, \
Apple holds the keys to those files. Audio, voiceprints, API keys and settings are never written there.
"""
