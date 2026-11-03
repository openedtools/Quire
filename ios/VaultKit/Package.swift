// swift-tools-version: 6.2
// The vault format (sync-format.md) for the iPhone app: the same rules and fixtures as the
// desktop's `src/vault/format.rs`.
import PackageDescription

let package = Package(
    name: "VaultKit",
    platforms: [.iOS(.v26), .macOS(.v15)],
    products: [
        .library(name: "VaultKit", targets: ["VaultKit"]),
        .library(name: "RecordingKit", targets: ["RecordingKit"]),
    ],
    targets: [
        .target(name: "VaultKit"),
        .testTarget(name: "VaultKitTests", dependencies: ["VaultKit"]),
        // Recording logic with no audio hardware or model: pause detection and the session saved
        // in the vault format as it goes (plan I2)
        .target(name: "RecordingKit", dependencies: ["VaultKit"]),
        .testTarget(name: "RecordingKitTests", dependencies: ["RecordingKit", "VaultKit"]),
    ]
)
