// Quire for iPhone (spec-ios.md): the vault reader, recording, and from I3 retention and the
// vault writer, run at launch, on return to the foreground and in a background task.

import BackgroundTasks
import SwiftUI

@main
struct QuireApp: App {
    @State private var vault: VaultStore
    @State private var models = ModelStore()
    @State private var housekeeper: Housekeeper
    @State private var assistant = Assistant()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        #if DEBUG
        // UI tests start each run with no saved choices, like a fresh install (debug builds only)
        if ProcessInfo.processInfo.environment["QUIRE_FRESH_DEFAULTS"] != nil, let id = Bundle.main.bundleIdentifier {
            UserDefaults.standard.removePersistentDomain(forName: id)
        }
        #endif
        let vault = VaultStore()
        let housekeeper = Housekeeper()
        _vault = State(initialValue: vault)
        _housekeeper = State(initialValue: housekeeper)
        // iOS gives no timer guarantee: this runs when iOS decides (PI17)
        BGTaskScheduler.shared.register(forTaskWithIdentifier: Housekeeper.taskID, using: nil) { task in
            nonisolated(unsafe) let task = task
            let work = Task { @MainActor in
                Housekeeper.scheduleBackground()
                await housekeeper.run(vault: vault)
                task.setTaskCompleted(success: !Task.isCancelled)
            }
            // iOS ends the time it gave: stop, and the next run carries on (every step is safe to repeat)
            task.expirationHandler = { work.cancel() }
        }
    }

    var body: some Scene {
        WindowGroup {
            // The app's own terracotta for links and buttons, never the system blue
            RootView().environment(vault).environment(models).environment(housekeeper).environment(assistant)
                .tint(Theme.accentStrong)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active: Task { await housekeeper.run(vault: vault) }
            case .background: Housekeeper.scheduleBackground()
            default: break
            }
        }
    }
}
