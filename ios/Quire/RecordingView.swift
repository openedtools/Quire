// The recording screen from the concept (plan I2): title and folder, a level waveform, the
// timer, pause, stop and bookmark, the live transcript, and the consent reminder at the start.

import RecordingKit
import SwiftUI
import UniformTypeIdentifiers
import VaultKit

/// The desktop's reminder wording (spec F-CON-1 to F-CON-4), word for word.
let consentReminderText = """
Rules about recording and transcribing conversations differ by place, and some can apply even when no audio is saved. \
A good habit: tell everyone, get a clear yes, and follow your organization's policy. This is general guidance, not legal advice.
"""

private let reminderKey = "show_recording_notification"

func reminderEnabled() -> Bool { UserDefaults.standard.object(forKey: reminderKey) as? Bool ?? true }

struct ConsentReminder: View {
    private var reminderText: some View {
        Text(consentReminderText).font(.footnote).foregroundStyle(Theme.inkMuted)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    @Binding var shown: Bool
    @State private var never = false
    @State private var readMore = false
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Tell people you're recording").font(.subheadline.weight(.semibold)).foregroundStyle(Theme.ink)
                Spacer()
                Button { shown = false } label: { Image(systemName: "xmark") }.accessibilityLabel("Close").tint(Theme.ink)
            }
            // As tall as the text needs; it scrolls only when large text would not fit
            ViewThatFits(in: .vertical) {
                reminderText
                ScrollView { reminderText }.frame(maxHeight: 320)
            }
            Toggle("Never show this again", isOn: $never).font(.footnote).tint(Theme.accentStrong)
            HStack {
                Button("Read more") { readMore = true }.font(.subheadline).tint(Theme.accentStrong)
                Spacer()
                Button("Got it") {
                    if never { UserDefaults.standard.set(false, forKey: reminderKey) }
                    shown = false
                }
                .buttonStyle(.borderedProminent).tint(Theme.accentStrong)
            }
        }
        .padding(16)
        .background(Theme.paper, in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Theme.lineStrong))
        .padding(.horizontal, 16)
        .accessibilityElement(children: .contain)
        .sheet(isPresented: $readMore) { NavigationStack { ConsentGuideView() } }
    }
}

struct RecordingView: View {
    @Environment(ModelStore.self) private var models
    @Environment(VaultStore.self) private var vault
    @Environment(\.dismiss) private var dismiss
    @State var recorder: Recorder
    let transcriptOnly: Bool
    let folderID: String?
    /// A recording of your own thoughts (PN4): no reminder, since it is just you
    var thoughts = false
    var onFinished: () -> Void
    @State private var showReminder = reminderEnabled()
    @State private var levels: [Float] = Array(repeating: 0, count: 40)
    @State private var jump: Int?

    var body: some View {
        VStack(spacing: 18) {
            HStack {
                Button {
                    // Stopping saves; when nothing was recorded (it failed to start) it just closes
                    if recorder.phase == .recording || recorder.phase == .paused {
                        Task { await recorder.stop() }
                    } else if recorder.phase != .finishing {
                        recorder.cancel()
                        dismiss()
                    }
                } label: { Image(systemName: "xmark").font(.title3) }
                    .tint(Theme.ink)
                    .accessibilityLabel("Stop and close")
                Spacer()
            }
            VStack(spacing: 4) {
                Text("New Recording").font(.serifTitle(26)).foregroundStyle(Theme.ink)
                Text(transcriptOnly ? "\(destination) · Transcript only" : destination).font(.subheadline).foregroundStyle(Theme.inkMuted)
            }
            waveform.frame(height: 70).accessibilityHidden(true)
            Text(clock(recorder.elapsed)).font(.title2.monospacedDigit()).foregroundStyle(Theme.ink)
                .accessibilityLabel("Recorded \(clock(recorder.elapsed))")
            controls
            transcript
            if let id = recorder.meetingID {
                AskPanel(meetingID: id, lines: recorder.lines, notes: recorder.notesText, backlog: recorder.backlogSeconds, heatHeld: recorder.heatHeld) { seconds in
                    jump = recorder.lines.lastIndex { $0.start <= seconds + 0.5 }
                }
            }
            if let note = recorder.interruptionNote { Text(note).font(.footnote).foregroundStyle(Theme.warning) }
            if let heat = recorder.heatNote {
                Label(recorder.phase == .finishing && recorder.minutesLeft > 0 ? "\(heat) About \(recorder.minutesLeft) min left." : heat, systemImage: "thermometer.medium")
                    .font(.footnote).foregroundStyle(Theme.warning)
            }
        }
        .padding(20)
        .background(alignment: .bottom) { RecordingHills().ignoresSafeArea() }
        .background(Theme.paper.ignoresSafeArea())
        .overlay(alignment: .bottom) {
            if showReminder { ConsentReminder(shown: $showReminder).padding(.bottom, 12) }
        }
        .task {
            if thoughts { showReminder = false }
            await recorder.start(
                title: thoughts ? defaultThoughtsTitle(Date()) : defaultRecordingTitle(Date()), transcriptOnly: transcriptOnly,
                modelDirectory: models.directory, retention: currentRetention(), folderID: folderID, kind: thoughts ? thoughtsKind : nil)
        }
        .onDisappear { recorder.cancel() }
        .onChange(of: recorder.level) { _, level in
            levels.removeFirst()
            levels.append(level)
        }
        .onChange(of: recorder.phase) { _, phase in
            if phase == .done {
                onFinished()
                dismiss()
            }
        }
    }

    /// Where the recording goes, like the concept's breadcrumb
    private var destination: String {
        guard vault.rootURL != nil else { return "On this iPhone" }
        return "Vault › " + (vault.contents.folders.first { $0.id == folderID }?.name ?? "Unfiled")
    }

    /// Sage bars, mirrored around the middle like the concept's waveform
    private var waveform: some View {
        HStack(alignment: .center, spacing: 4) {
            ForEach(Array(levels.enumerated()), id: \.offset) { index, level in
                let edge = 1 - abs(Double(index) - Double(levels.count - 1) / 2) / Double(levels.count)
                Capsule().fill(Theme.sage.opacity(0.85))
                    .frame(width: 3, height: max(6, CGFloat(min(level * 4, 1)) * 70 * edge))
            }
        }
        .animation(.easeOut(duration: 0.12), value: levels)
    }

    private var controls: some View {
        HStack(spacing: 28) {
            Button {
                recorder.phase == .paused ? recorder.resume() : recorder.pause()
            } label: {
                Image(systemName: recorder.phase == .paused ? "play.fill" : "pause.fill").font(.title2).frame(width: 60, height: 60)
                    .background(Theme.surface, in: Circle())
            }
            .accessibilityLabel(recorder.phase == .paused ? "Resume" : "Pause")
            Button {
                Task { await recorder.stop() }
            } label: {
                Image(systemName: "stop.fill").font(.title).foregroundStyle(Theme.onAccent).frame(width: 78, height: 78)
                    .background(Theme.accentStrong, in: Circle())
            }
            .accessibilityLabel("Stop")
            Button { recorder.bookmark() } label: {
                Image(systemName: "bookmark").font(.title2).frame(width: 60, height: 60).background(Theme.surface, in: Circle())
            }
            .accessibilityLabel("Bookmark this moment")
        }
        .tint(Theme.ink)
        .disabled(recorder.phase != .recording && recorder.phase != .paused)
    }

    private var transcript: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if recorder.lines.isEmpty {
                        Text(statusText).foregroundStyle(Theme.inkMuted)
                    }
                    ForEach(Array(recorder.lines.enumerated()), id: \.offset) { index, line in
                        HStack(alignment: .top, spacing: 8) {
                            Text(clock(line.start)).font(.caption.monospacedDigit()).foregroundStyle(Theme.inkMuted)
                            Text(line.text).foregroundStyle(Theme.ink)
                        }
                        .id(index)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: recorder.lines.count) { _, count in proxy.scrollTo(count - 1, anchor: .bottom) }
            .onChange(of: jump) { _, index in
                guard let index else { return }
                withAnimation { proxy.scrollTo(index, anchor: .center) }
                jump = nil
            }
        }
    }

    private var statusText: String {
        switch recorder.phase {
        case let .failed(message): return message
        case .finishing: return "Finishing the transcript"
        default: return "The transcript appears here after each pause."
        }
    }
}

/// Starting a recording: the transcript-only choice, and the model download when it is missing.
struct RecordStartView: View {
    @Environment(ModelStore.self) private var models
    @Environment(VaultStore.self) private var vault
    /// The folder used last, as one-tap record uses (plan PN3)
    @State private var folderID: String? = UserDefaults.standard.string(forKey: lastFolderKey)
    @Environment(\.dismiss) private var dismiss
    @AppStorage("transcriptOnly") private var transcriptOnly = false
    var onStart: (Bool, String?) -> Void
    var onImport: (URL, Bool, String?) -> Void
    @State private var importing = false
    @State private var askCellular = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("Transcript only: do not save audio", isOn: $transcriptOnly).tint(Theme.accentStrong)
                } footer: {
                    Text("Transcript only saves no audio file. Whether you need people's consent depends on where you and they are, and it does not change with this setting.")
                }
                Section {
                    if vault.rootURL != nil {
                        Picker("Save in", selection: $folderID) {
                            Text("Unfiled").tag(String?.none)
                            ForEach(vault.contents.folders, id: \.id) { Text($0.name).tag(Optional($0.id)) }
                        }
                    } else {
                        Text("On this iPhone").foregroundStyle(Theme.ink)
                    }
                } header: {
                    Text("Where")
                } footer: {
                    Text(vault.rootURL != nil
                        ? "The recording moves into the vault when it ends. \(retentionSentence(currentRetention())) Change this in Settings."
                        : "Recordings move into the vault once you choose its folder in Settings.")
                }
                Section("Transcription") { ModelRow(askCellular: $askCellular) }
                Section {
                    Button("Start recording") {
                        onStart(transcriptOnly, folderID)
                        dismiss()
                    }
                    .disabled(models.state != .ready)
                    Button("Import a recording") { importing = true }.disabled(models.state != .ready)
                } footer: {
                    Text("Recording and transcription happen on this iPhone.")
                }
            }
            .navigationTitle("Record")
            .onAppear {
                // A folder that has gone since is not offered
                if vault.state == .ready, let id = folderID, !vault.contents.folders.contains(where: { $0.id == id }) { folderID = nil }
            }
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.audio]) { result in
                if case let .success(url) = result {
                    onImport(url, transcriptOnly, folderID)
                    dismiss()
                }
            }
        }
    }
}

/// Parakeet v3: size, languages, and a download that asks first (plan PI8).
struct ModelRow: View {
    @Environment(ModelStore.self) private var models
    @Binding var askCellular: Bool
    @State private var askDownload = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Parakeet v3").foregroundStyle(Theme.ink)
            Text("25 European languages and Japanese. Runs on this iPhone.").font(.footnote).foregroundStyle(Theme.inkMuted)
            switch models.state {
            case .ready:
                Label("Downloaded and checked", systemImage: "checkmark.seal").font(.footnote).foregroundStyle(Theme.sage)
            case let .downloading(done):
                ProgressView(value: done) { Text("Downloading \(Int(done * 100))%").font(.footnote) }
            case let .failed(message):
                Text(message).font(.footnote).foregroundStyle(Theme.warning)
                Button("Try again") { askDownload = true }
            case .missing:
                Button("Download (\(models.sizeText))") { askDownload = true }
            }
        }
        .alert("Download the transcription model?", isPresented: $askDownload) {
            Button("Cancel", role: .cancel) {}
            Button("Download") {
                Task {
                    if await ModelStore.onExpensiveNetwork() { askCellular = true } else { models.download() }
                }
            }
        } message: {
            Text("\(models.sizeText) from Hugging Face, checked file by file before use. Nothing else is sent.")
        }
        .alert("You are on mobile data", isPresented: $askCellular) {
            Button("Wait for Wi-Fi", role: .cancel) {}
            Button("Download anyway") { models.download() }
        } message: {
            Text("The model is \(models.sizeText). Downloading it may use your data plan.")
        }
    }
}
