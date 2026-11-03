// A meeting's audio on the phone (spec F-RET-8, 9; plan PI15, PI16): its status in words, "Keep
// this one" and "Delete audio now" for the phone's own meetings, and playback while the audio is
// kept. Desktop meetings show their status only (read-only, F-IOS-VAULT-4).

import AVFoundation
import Observation
import RecordingKit
import SwiftUI
import VaultKit

/// "Audio kept until 9 Nov", "Audio deleted on 8 Nov", "No audio saved" (F-RET-9). While this
/// iPhone still holds the file (waiting for the vault, paused, or a deletion to retry), it says
/// "deletion pending" rather than claiming it is gone (F-RET-6).
func audioStatusText(_ note: Note, now: Date = Date(), fileStillHere: Bool = false) -> String {
    let day = Date.FormatStyle(date: .abbreviated, time: .omitted)
    let status = audioStatus(note.audio, ended: note.ended, now: now)
    switch status {
    case .expired, .deleted: if fileStillHere { return "Audio deletion pending" }
    default: break
    }
    switch status {
    case .none: return "No audio saved"
    case let .kept(until): return until.map { "Audio kept until \($0.formatted(day))" } ?? "Audio kept"
    case let .expired(at): return "Audio deleted on \(at.formatted(day))"
    case let .deleted(at): return at.map { "Audio deleted on \($0.formatted(day))" } ?? "Audio deleted"
    }
}

@MainActor
@Observable
final class AudioPlayback {
    private var player: AVAudioPlayer?
    private var timer: Task<Void, Never>?
    private(set) var playing = false
    private(set) var duration: Double = 0
    var position: Double = 0
    var rate: Float = 1 {
        didSet { player?.rate = rate }
    }

    init(url: URL) {
        player = try? AVAudioPlayer(contentsOf: url)
        player?.enableRate = true
        duration = player?.duration ?? 0
    }

    var available: Bool { player != nil }

    func toggle() {
        guard let player else { return }
        if playing {
            player.pause()
            playing = false
            timer?.cancel()
        } else {
            try? AVAudioSession.sharedInstance().setCategory(.playback)
            try? AVAudioSession.sharedInstance().setActive(true)
            player.rate = rate
            player.play()
            playing = true
            timer = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(250))
                    guard let self, let player = self.player else { return }
                    self.position = player.currentTime
                    if !player.isPlaying {
                        self.playing = false
                        return
                    }
                }
            }
        }
    }

    func seek(to seconds: Double) {
        player?.currentTime = seconds
        position = seconds
    }

    func stop() {
        player?.stop()
        playing = false
        timer?.cancel()
    }
}

struct AudioSection: View {
    @Environment(VaultStore.self) private var vault
    @Environment(Housekeeper.self) private var housekeeper
    let note: Note
    @State private var confirmDelete = false
    @State private var playback: AudioPlayback?

    /// This iPhone's copy: `.aac` from I5 on, `.m4a` for recordings made before
    private var localAudio: URL? {
        [RecordingAudio.fileExtension, "m4a"].lazy.map { localVaultFolder().appendingPathComponent("\(note.id).\($0)") }
            .first { FileManager.default.fileExists(atPath: $0.path) }
    }

    var body: some View {
        let status = audioStatus(note.audio, ended: note.ended, now: Date())
        let mine = housekeeper.isPhoneMeeting(note)
        VStack(alignment: .leading, spacing: 10) {
            Label(audioStatusText(note, fileStillHere: localAudio != nil), systemImage: "waveform").font(.subheadline).foregroundStyle(Theme.inkMuted)
            if case let .kept(until) = status, mine {
                if let playback, playback.available { player(playback) }
                HStack(spacing: 12) {
                    if until != nil {
                        Button("Keep this one") {
                            Task { await housekeeper.change(note.id, vault: vault) { keepAudio(&$0, now: Date(), deviceID: phoneDeviceID()) } }
                        }
                    }
                    Button("Delete audio now", role: .destructive) { confirmDelete = true }
                }
                .buttonStyle(.bordered)
                .font(.subheadline)
            }
        }
        .onAppear {
            if playback == nil, let url = localAudio, case .kept = status { playback = AudioPlayback(url: url) }
        }
        .onDisappear { playback?.stop() }
        .confirmationDialog("Delete the audio of this meeting?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete audio", role: .destructive) {
                playback?.stop()
                playback = nil
                Task { await housekeeper.change(note.id, vault: vault) { deleteAudioNow(&$0, now: Date(), deviceID: phoneDeviceID()) } }
            }
        } message: {
            Text("The transcript and notes stay. iCloud's Recently Deleted and backups are outside Quire's control.")
        }
    }

    /// The concept's player: the scrubber with its times, then speed, a large play button and back 15 seconds
    private func player(_ playback: AudioPlayback) -> some View {
        VStack(spacing: 10) {
            Slider(value: Binding(get: { playback.position }, set: { playback.seek(to: $0) }), in: 0...max(playback.duration, 1))
                .tint(Theme.accent)
                .accessibilityLabel("Position")
                .accessibilityValue(clock(playback.position))
            HStack {
                Text(clock(playback.position))
                Spacer()
                Text(clock(playback.duration))
            }
            .font(.caption.monospacedDigit()).foregroundStyle(Theme.inkMuted)
            HStack {
                Menu {
                    ForEach([Float(1), 1.25, 1.5, 2], id: \.self) { r in Button("\(r.formatted())×") { playback.rate = r } }
                } label: {
                    Text("\(playback.rate.formatted(.number.precision(.fractionLength(1))))×").font(.subheadline.weight(.medium))
                        .frame(width: 56, height: 44).background(Theme.surface, in: Capsule())
                }
                .accessibilityLabel("Speed")
                Spacer()
                Button { playback.toggle() } label: {
                    Image(systemName: playback.playing ? "pause.fill" : "play.fill").font(.title2)
                        .frame(width: 66, height: 66)
                        .background(Theme.surface, in: Circle())
                        .overlay(Circle().stroke(Theme.line))
                }
                .accessibilityLabel(playback.playing ? "Pause" : "Play")
                Spacer()
                Button { playback.seek(to: max(0, playback.position - 15)) } label: {
                    Image(systemName: "gobackward.15").font(.title3).frame(width: 56, height: 44)
                }
                .accessibilityLabel("Back 15 seconds")
            }
        }
        .tint(Theme.ink)
        .padding(.vertical, 6)
    }
}
