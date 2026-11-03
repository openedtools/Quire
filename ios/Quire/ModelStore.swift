// The Parakeet v3 model for transcription: only the files needed, from a pinned revision, each
// checked by size and SHA-256 before it is used (plan PI8). The only network request the app
// makes, and only when the person asks for it.

import CryptoKit
import Foundation
import Network
import Observation

struct ModelFile: Decodable, Sendable {
    let path: String
    let bytes: Int64
    let sha256: String
}

struct ModelManifest: Decodable, Sendable {
    let repo: String
    let revision: String
    let files: [ModelFile]

    static let parakeet: ModelManifest = {
        let url = Bundle.main.url(forResource: "ParakeetModel", withExtension: "json")!
        return try! JSONDecoder().decode(ModelManifest.self, from: Data(contentsOf: url))
    }()

    var totalBytes: Int64 { files.reduce(0) { $0 + $1.bytes } }
}

/// SHA-256 of a file read in 1 MB pieces (the encoder alone is about 450 MB).
func sha256(of url: URL) throws -> String {
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    var hasher = SHA256()
    while let chunk = try handle.read(upToCount: 1 << 20), !chunk.isEmpty {
        hasher.update(data: chunk)
    }
    return hasher.finalize().map { String(format: "%02x", $0) }.joined()
}

@MainActor
@Observable
final class ModelStore {
    enum State: Equatable {
        case missing
        case downloading(Double)
        case ready
        case failed(String)
    }

    private(set) var state: State = .missing
    let manifest = ModelManifest.parakeet
    private var task: Task<Void, Never>?

    /// Tests and the simulator point at a folder that already holds the files.
    var directory: URL {
        if let path = ProcessInfo.processInfo.environment["QUIRE_MODEL_DIR"] { return URL(fileURLWithPath: path) }
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("models/parakeet-v3/\(manifest.revision)")
    }

    private var verifiedMarker: URL { directory.appendingPathComponent(".verified") }

    init() {
        if FileManager.default.fileExists(atPath: verifiedMarker.path) || ProcessInfo.processInfo.environment["QUIRE_MODEL_DIR"] != nil {
            state = .ready
        }
    }

    var sizeText: String { ByteCountFormatter.string(fromByteCount: manifest.totalBytes, countStyle: .file) }

    /// True on mobile data or a hotspot, where the person is asked again (PI8).
    static func onExpensiveNetwork() async -> Bool {
        await withCheckedContinuation { continuation in
            let monitor = NWPathMonitor()
            monitor.pathUpdateHandler = { path in
                monitor.cancel()
                continuation.resume(returning: path.isExpensive)
            }
            monitor.start(queue: DispatchQueue(label: "quire.network-check"))
        }
    }

    func download() {
        guard task == nil else { return }
        state = .downloading(0)
        let manifest = manifest
        let directory = directory
        task = Task {
            do {
                try await Self.fetch(manifest: manifest, into: directory) { done in
                    Task { @MainActor in self.state = .downloading(done) }
                }
                try Data().write(to: directory.appendingPathComponent(".verified"))
                state = .ready
            } catch {
                state = .failed("The model could not be downloaded or did not match its fingerprint. Try again on Wi-Fi.")
            }
            task = nil
        }
    }

    /// Downloads each missing or wrong file and checks it; a file that does not match is deleted.
    nonisolated static func fetch(manifest: ModelManifest, into directory: URL, progress: @escaping @Sendable (Double) -> Void) async throws {
        let fm = FileManager.default
        var done: Int64 = 0
        for file in manifest.files {
            let target = directory.appendingPathComponent(file.path)
            try fm.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            let size = (try? fm.attributesOfItem(atPath: target.path)[.size] as? Int64) ?? -1
            if size != file.bytes || (try? sha256(of: target)) != file.sha256 {
                try? fm.removeItem(at: target)
                let url = URL(string: "https://huggingface.co/\(manifest.repo)/resolve/\(manifest.revision)/\(file.path)")!
                let (temp, response) = try await URLSession.shared.download(from: url)
                guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw URLError(.badServerResponse) }
                try fm.moveItem(at: temp, to: target)
                let bytes = (try fm.attributesOfItem(atPath: target.path)[.size] as? Int64) ?? -1
                guard bytes == file.bytes, try sha256(of: target) == file.sha256 else {
                    try? fm.removeItem(at: target)
                    throw URLError(.cannotDecodeContentData)
                }
            }
            done += file.bytes
            progress(Double(done) / Double(manifest.totalBytes))
        }
    }
}
