// The I1 screens: Home (record button and folders), Transcripts, Library (people), a meeting,
// and Settings: the vault folder, audio retention and the model. Desktop meetings are read-only.

import RecordingKit
import SwiftUI
import UniformTypeIdentifiers
import VaultKit

// ===== Formatting =====

func displayDate(_ iso: String) -> String {
    guard let date = try? Date(iso, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)) else { return "" }
    return date.formatted(date: .abbreviated, time: .omitted)
}

func clock(_ seconds: Double) -> String {
    // Never trust a number from the vault: non-finite or huge values show as 00:00
    guard seconds.isFinite, seconds >= 0, seconds <= maxSeconds else { return "00:00" }
    let s = Int(seconds.rounded(.down))
    return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60) : String(format: "%02d:%02d", s / 60, s % 60)
}

/// Markdown from the vault, shown as text: inline styles only, links never live (untrusted input).
func safeMarkdown(_ text: String) -> AttributedString {
    var attributed = (try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text)
    for run in attributed.runs where run.link != nil {
        attributed[run.range].link = nil
    }
    return attributed
}

/// Markdown blocks from the vault or the model, drawn safely: headings, bold section lines,
/// lists and table rows become plain styled text; inline styles stay; links are never live;
/// images and HTML never render (untrusted input).
struct MarkdownBlocks: View {
    let text: String

    private enum Block: Hashable {
        case heading(String, Int)
        case bullet(String)
        case row([String])
        case paragraph(String)
    }

    private var blocks: [Block] {
        var out: [Block] = []
        var paragraph: [String] = []
        func flush() {
            if !paragraph.isEmpty { out.append(.paragraph(paragraph.joined(separator: " "))) }
            paragraph = []
        }
        for raw in text.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty { flush(); continue }
            if let hashes = line.firstIndex(where: { $0 != "#" }), line.hasPrefix("#"), line[hashes] == " " {
                flush()
                out.append(.heading(String(line[hashes...]).trimmingCharacters(in: .whitespaces), line.distance(from: line.startIndex, to: hashes)))
            } else if line.hasPrefix("**"), line.hasSuffix("**"), line.count > 4, !line.dropFirst(2).dropLast(2).contains("**") {
                flush()
                out.append(.heading(String(line.dropFirst(2).dropLast(2)), 3))
            } else if line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("+ ") {
                flush()
                out.append(.bullet(String(line.dropFirst(2))))
            } else if line.hasPrefix("|") {
                flush()
                let cells = line.split(separator: "|").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                // The separator row under a table header carries no content
                if !cells.allSatisfy({ $0.allSatisfy { "-: ".contains($0) } }) { out.append(.row(cells)) }
            } else {
                paragraph.append(line)
            }
        }
        flush()
        return out
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case let .heading(title, level):
                    Text(safeMarkdown(title)).font(level <= 1 ? .title3.weight(.semibold) : .headline).padding(.top, 4)
                        .accessibilityAddTraits(.isHeader)
                case let .bullet(item):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("•").accessibilityHidden(true)
                        Text(safeMarkdown(item))
                    }
                case let .row(cells):
                    Text(safeMarkdown(cells.joined(separator: " · "))).font(.subheadline)
                case let .paragraph(body):
                    Text(safeMarkdown(body))
                }
            }
        }
        .foregroundStyle(Theme.ink)
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// ===== The app =====

let startedBeforeKey = "startedARecordingBefore"
let lastFolderKey = "lastRecordingFolderID"

struct RootView: View {
    @Environment(VaultStore.self) private var vault
    @Environment(ModelStore.self) private var models
    @State private var showSettings = false
    @State private var showStart = false
    @Environment(Housekeeper.self) private var housekeeper
    @Environment(Assistant.self) private var assistant
    @State private var recording: (Recorder, Bool, String?, Bool)?
    @State private var importNote: String?
    private let transcriber = Transcriber()

    /// One tap records (plan PN3): with the choices used last, once the speech model is ready and
    /// the choices have been seen once. Otherwise the options open first.
    private func record() {
        let defaults = UserDefaults.standard
        guard defaults.bool(forKey: startedBeforeKey), models.state == .ready else {
            showStart = true
            return
        }
        // A folder that has gone since falls back to no folder; while the vault is still being
        // read the folder is kept, and the move into the vault copes if it has gone
        let folderID = defaults.string(forKey: lastFolderKey).flatMap { id in
            vault.rootURL != nil && (vault.state != .ready || vault.contents.folders.contains { $0.id == id }) ? id : nil
        }
        recording = (Recorder(transcriber: transcriber), defaults.bool(forKey: "transcriptOnly"), folderID, false)
    }

    /// "Record my thoughts" (plan PN4): at once, filed in a Thoughts folder, made if needed.
    private func recordThoughts() {
        guard models.state == .ready else {
            showStart = true
            return
        }
        Task {
            var folderID = thoughtsFolder()
            if vault.rootURL != nil, folderID == nil {
                // The new folder's ID, not a reread of the vault, which may not have finished
                let device = phoneDeviceID()
                folderID = await housekeeper.folderWork(vault: vault) { writer in
                    try writer.createFolder(named: thoughtsFolderName, deviceID: device, now: Date()).id
                }
            }
            recording = (Recorder(transcriber: transcriber), UserDefaults.standard.bool(forKey: "transcriptOnly"), folderID, true)
        }
    }

    private func thoughtsFolder() -> String? {
        vault.contents.folders.first { $0.name.compare(thoughtsFolderName, options: .caseInsensitive) == .orderedSame }?.id
    }

    var body: some View {
        TabView {
            Tab("Home", systemImage: "house") { NavigationStack { HomeView(showSettings: $showSettings, showStart: $showStart, importNote: importNote, onRecord: record, onThoughts: recordThoughts) } }
            Tab("Meetings", systemImage: "folder") { NavigationStack { MeetingsView() } }
            Tab("People", systemImage: "person.2") { NavigationStack { PeopleView() } }
        }
        .tint(Theme.accentStrong)
        .sheet(isPresented: $showSettings) { SettingsView() }
        .sheet(isPresented: $showStart) {
            RecordStartView { transcriptOnly, folderID in
                UserDefaults.standard.set(true, forKey: startedBeforeKey)
                UserDefaults.standard.set(folderID, forKey: lastFolderKey)
                recording = (Recorder(transcriber: transcriber), transcriptOnly, folderID, false)
            } onImport: { url, transcriptOnly, folderID in
                importNote = "Transcribing the imported recording"
                Task {
                    let directory = models.directory
                    do {
                        _ = try await Recorder.importRecording(
                            from: url, transcriptOnly: transcriptOnly, transcriber: transcriber, modelDirectory: directory,
                            retention: currentRetention(), folderID: folderID)
                        importNote = nil
                    } catch {
                        importNote = "That recording could not be imported."
                    }
                    await housekeeper.run(vault: vault)
                }
            }
        }
        .fullScreenCover(isPresented: Binding(get: { recording != nil }, set: { if !$0 { recording = nil } })) {
            if let (recorder, transcriptOnly, folderID, thoughts) = recording {
                RecordingView(recorder: recorder, transcriptOnly: transcriptOnly, folderID: folderID, thoughts: thoughts) {
                    Task {
                        await housekeeper.run(vault: vault)
                        // A name, then a summary, once the last line is in (PL1, PI19), unless turned off in Settings
                        if let id = recorder.meetingID, !recorder.lines.isEmpty {
                            await assistant.nameAfterRecording(id, vault: vault, housekeeper: housekeeper)
                        }
                        if autoSummaryOn(), let id = recorder.meetingID, !recorder.lines.isEmpty {
                            assistant.summarize(id, vault: vault, housekeeper: housekeeper)
                        }
                    }
                }
            }
        }
        .alert("Delete audio for \(housekeeper.heldBack.count) recordings?", isPresented: Binding(
            get: { !housekeeper.heldBack.isEmpty }, set: { _ in })
        ) {
            Button("Not now", role: .cancel) { housekeeper.postpone() }
            Button("Delete", role: .destructive) {
                let asked = Set(housekeeper.heldBack)
                Task { await housekeeper.run(vault: vault, confirmed: asked) }
            }
        } message: {
            Text("Their retention time has passed. This many at once can mean the clock or a vault file is wrong, so Quire asks first. Transcripts stay.")
        }
    }
}

struct HomeView: View {
    @Environment(VaultStore.self) private var vault
    @Environment(Housekeeper.self) private var housekeeper
    @Binding var showSettings: Bool
    @Binding var showStart: Bool
    var importNote: String?
    var onRecord: () -> Void = {}
    var onThoughts: () -> Void = {}

    var body: some View {
        ScrollView {
            VStack(spacing: 28) {
                Text("Record & Transcribe").font(.serifTitle()).foregroundStyle(Theme.ink).padding(.top, 56)
                VStack(spacing: 12) {
                    Button(action: onRecord) {
                        ZStack {
                            PulseRings()
                            Circle().fill(Theme.accent.opacity(0.16)).frame(width: 136, height: 136)
                            Circle().stroke(Theme.accent.opacity(0.25), lineWidth: 1).frame(width: 136, height: 136)
                            Circle().fill(Theme.accent).frame(width: 108, height: 108)
                                .shadow(color: Theme.accentStrong.opacity(0.25), radius: 10, y: 4)
                            Image(systemName: "mic").font(.system(size: 40, weight: .regular)).foregroundStyle(Theme.onAccent)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Record")
                    .accessibilityHint("Starts recording at once with your last choices")
                    // A long press, or the link below, opens the choices (plan PN3)
                    .simultaneousGesture(LongPressGesture(minimumDuration: 0.5).onEnded { _ in showStart = true })
                    .accessibilityAction(named: "Recording options") { showStart = true }
                    Text(importNote ?? "Tap to record").font(.subheadline).foregroundStyle(Theme.inkMuted)
                    HStack(spacing: 18) {
                        Button("Recording options") { showStart = true }
                        Button("Record my thoughts", systemImage: "lightbulb", action: onThoughts)
                    }
                    .font(.footnote).foregroundStyle(Theme.accentStrong)
                }
                if !vault.local.meetings.isEmpty {
                    NavigationLink {
                        MeetingList(title: "On this iPhone", meetings: vault.local.meetings)
                    } label: {
                        FolderRow(name: "On this iPhone", count: vault.local.meetings.count)
                    }
                }
                if let notice = housekeeper.notice {
                    Card {
                        HStack(alignment: .top) {
                            Text(notice).font(.footnote).foregroundStyle(Theme.ink)
                            Spacer()
                            Button { housekeeper.dismissNotice() } label: { Image(systemName: "xmark") }.accessibilityLabel("Dismiss")
                        }
                    }
                }
                vaultSection
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 24)
        }
        .background(alignment: .top) { HomeLandscape().ignoresSafeArea() }
        .background(Theme.paper)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showSettings = true } label: { Image(systemName: "gearshape") }.accessibilityLabel("Settings")
            }
        }
    }

    @ViewBuilder private var vaultSection: some View {
        switch vault.state {
        case .none:
            Card {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Your desktop meetings").font(.headline).foregroundStyle(Theme.ink)
                    Text("Choose the Quire vault folder in iCloud Drive to see the meetings the desktop app writes.").foregroundStyle(Theme.inkMuted)
                    Button("Choose the vault folder") { showSettings = true }.buttonStyle(.borderedProminent).tint(Theme.accentStrong)
                }
            }
        case .loading:
            ProgressView("Reading the vault")
        case let .failed(message):
            Card { Text(message).foregroundStyle(Theme.ink) }
        case .ready:
            VStack(spacing: 10) {
                ForEach(vault.contents.folders, id: \.id) { folder in
                    NavigationLink {
                        MeetingList(title: folder.name, meetings: vault.meetings(in: folder.id))
                    } label: {
                        FolderRow(name: folder.name, count: vault.meetings(in: folder.id).count)
                    }
                }
                let unfiled = vault.meetings(in: nil)
                if !unfiled.isEmpty {
                    NavigationLink { MeetingList(title: "Unfiled", meetings: unfiled) } label: { FolderRow(name: "Unfiled", count: unfiled.count) }
                }
                if vault.contents.newerFormat {
                    Text("This vault was made by a newer version of Quire. Update the app to edit it.").font(.footnote).foregroundStyle(Theme.warning)
                }
                if vault.contents.waitingForICloud > 0 {
                    Text("Waiting for iCloud to download \(vault.contents.waitingForICloud) file(s)").font(.footnote).foregroundStyle(Theme.inkMuted)
                }
            }
        }
    }
}

struct FolderRow: View {
    let name: String
    let count: Int
    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: "folder.fill").font(.title2).foregroundStyle(Theme.accent.opacity(0.42)).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(name).font(.body.weight(.medium)).foregroundStyle(Theme.ink)
                Text("\(count) item\(count == 1 ? "" : "s")").font(.subheadline).foregroundStyle(Theme.inkMuted)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.subheadline.weight(.semibold)).foregroundStyle(Theme.inkMuted).accessibilityHidden(true)
        }
        .padding(.horizontal, 18).padding(.vertical, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surface.opacity(0.7), in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).stroke(Theme.line))
        .shadow(color: Theme.ink.opacity(0.05), radius: 8, y: 3)
        .accessibilityElement(children: .combine)
    }
}

/// Every meeting (plan PN1): folders first, then meetings in no folder, newest first, with search
/// over names and folders. Meetings still waiting to move into the vault come first.
struct MeetingsView: View {
    @Environment(VaultStore.self) private var vault
    @Environment(Housekeeper.self) private var housekeeper
    @State private var query = ""
    /// The folder being named: "" for a new one, or the ID of the one being renamed
    @State private var naming: String?
    @State private var folderName = ""

    private var waiting: [VaultMeeting] {
        let inVault = Set(vault.contents.meetings.map(\.id))
        return newestFirst(vault.local.meetings.filter { !inVault.contains($0.id) })
    }

    var body: some View {
        List {
            if query.trimmingCharacters(in: .whitespaces).isEmpty {
                if !waiting.isEmpty {
                    Section("On this iPhone") { ForEach(waiting) { MeetingRow(meeting: $0) } }
                }
                if !vault.contents.folders.isEmpty {
                    Section {
                        ForEach(vault.contents.folders, id: \.id) { folder in
                            NavigationLink {
                                MeetingList(title: folder.name, meetings: newestFirst(vault.meetings(in: folder.id)))
                            } label: {
                                LabeledContent {
                                    Text("\(vault.meetings(in: folder.id).count)").foregroundStyle(Theme.inkMuted)
                                } label: {
                                    Label(folder.name, systemImage: "folder").foregroundStyle(Theme.ink)
                                }
                            }
                            .listRowBackground(Theme.surface)
                            .contextMenu {
                                if folder.updatedBy == phoneDeviceID() {
                                    Button("Rename", systemImage: "pencil") {
                                        folderName = folder.name
                                        naming = folder.id
                                    }
                                }
                            }
                        }
                    } header: {
                        Text("Folders")
                    } footer: {
                        Text("Folders made on this iPhone are in the vault folder too; the desktop shows them once two-way sync arrives.")
                    }
                }
                let unfiled = newestFirst(vault.meetings(in: nil))
                if !unfiled.isEmpty {
                    Section("Not in a folder") { ForEach(unfiled) { MeetingRow(meeting: $0) } }
                }
            } else {
                Section { ForEach(matches) { MeetingRow(meeting: $0) } }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.paper)
        .navigationTitle("Meetings")
        .searchable(text: $query, prompt: "Search meetings")
        .toolbar {
            if vault.rootURL != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        folderName = ""
                        naming = ""
                    } label: {
                        Label("New folder", systemImage: "folder.badge.plus")
                    }
                }
            }
        }
        .alert(naming == "" ? "New folder" : "Rename folder", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } })) {
            TextField("Name", text: $folderName)
            Button("Cancel", role: .cancel) {}
            Button(naming == "" ? "Make folder" : "Rename") {
                let (id, name, now, device) = (naming ?? "", folderName, Date(), phoneDeviceID())
                Task {
                    await housekeeper.folderWork(vault: vault) { writer in
                        if id.isEmpty {
                            _ = try writer.createFolder(named: name, deviceID: device, now: now)
                        } else {
                            let renamed = try writer.renameFolder(id, to: name, deviceID: device, now: now)
                            // The phone's notes in it now live under the new name
                            var bases = BaseStore(file: baseStoreURL())
                            try bases.rebase(dir: renamed.from, to: renamed.to)
                        }
                    }
                }
            }
        }
        .overlay {
            if vault.contents.meetings.isEmpty && waiting.isEmpty {
                ContentUnavailableView("No meetings yet", systemImage: "folder", description: Text("Your recordings and the desktop's meetings appear here."))
            } else if !query.isEmpty && matches.isEmpty {
                ContentUnavailableView.search(text: query)
            }
        }
    }

    private var matches: [VaultMeeting] {
        let words = query.lowercased().split(separator: " ")
        let folderName = Dictionary(vault.contents.folders.map { ($0.id, $0.name.lowercased()) }, uniquingKeysWith: { a, _ in a })
        return newestFirst((waiting + vault.contents.meetings).filter { m in
            let text = m.note.title.lowercased() + " " + (m.folderID.flatMap { folderName[$0] } ?? "")
            return words.allSatisfy { text.contains($0) }
        })
    }
}

func newestFirst(_ meetings: [VaultMeeting]) -> [VaultMeeting] {
    meetings.sorted { ($0.note.started ?? $0.note.created) > ($1.note.started ?? $1.note.created) }
}

struct MeetingRow: View {
    let meeting: VaultMeeting
    var body: some View {
        NavigationLink { MeetingView(meeting: meeting) } label: {
            VStack(alignment: .leading, spacing: 4) {
                Text(meeting.note.title).foregroundStyle(Theme.ink)
                Text(subtitle(meeting.note)).font(.footnote).foregroundStyle(Theme.inkMuted)
            }
        }
        .listRowBackground(Theme.surface)
    }
}

struct MeetingList: View {
    let title: String
    let meetings: [VaultMeeting]
    var body: some View {
        List(meetings) { MeetingRow(meeting: $0) }
        .scrollContentBackground(.hidden)
        .background(Theme.paper)
        .navigationTitle(title)
        .overlay {
            if meetings.isEmpty { ContentUnavailableView("No meetings", systemImage: "doc.text", description: Text("Meetings from the desktop vault appear here.")) }
        }
    }
}

func subtitle(_ note: Note) -> String {
    let date = displayDate(note.created)
    guard let d = note.durationS else { return date }
    return "\(date) · \(clock(Double(d)))"
}

struct MeetingView: View {
    @Environment(VaultStore.self) private var vault
    @Environment(Housekeeper.self) private var housekeeper
    @Environment(Assistant.self) private var assistant
    let meeting: VaultMeeting
    @State private var tab = 0
    @State private var jump: Int?
    @State private var asking = false
    @State private var suggested: String?
    @State private var suggesting = false

    /// The meeting as it is now (a summary or a move into the vault changes it)
    private var current: VaultMeeting {
        vault.local.meetings.first { $0.id == meeting.id } ?? vault.contents.meetings.first { $0.id == meeting.id } ?? meeting
    }

    private var mine: Bool { housekeeper.isPhoneMeeting(current.note) }

    private func move(to folderID: String?) {
        let id = current.id
        Task { await housekeeper.move(id, toFolder: folderID, vault: vault) }
    }

    private func suggestName() {
        suggesting = true
        let meeting = current
        let folder = vault.contents.folders.first { $0.id == meeting.folderID }?.name ?? ""
        Task {
            defer { suggesting = false }
            do {
                if let name = try await assistant.suggestName(meeting, folder: folder) { suggested = name }
                else { housekeeper.notice = "No name could be suggested from this transcript." }
            } catch {
                housekeeper.notice = assistProblem(error)
            }
        }
    }

    private var speakers: [Int64: String] {
        Dictionary(current.note.speakers.map { ($0.slot, speakerName($0.slot)) }, uniquingKeysWith: { a, _ in a })
    }

    /// The text of the open tab, for one-tap copy
    private var sectionText: (text: String, what: String) {
        switch tab {
        case 0: (transcriptText(current.segments ?? [], speakerName: { $0 == nil ? nil : speakerName($0) }), "transcript")
        case 1: (bodySections(current.note.body).summary ?? "", isThoughts(current.note) ? "prompt" : "summary")
        default: (bodySections(current.note.body).notes ?? "", "notes")
        }
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(current.note.title).font(.serifTitle(28)).foregroundStyle(Theme.ink)
                        Text(subtitle(current.note)).font(.subheadline).foregroundStyle(Theme.inkMuted)
                    }
                    AudioSection(note: current.note)
                    SectionTabs(selection: $tab, titles: ["Transcript", isThoughts(current.note) ? "Prompt" : "Summary", "Notes"])
                    HStack {
                        Spacer()
                        CopyButton(text: sectionText.text, what: sectionText.what)
                    }
                    switch tab {
                    case 0: transcript
                    case 1: summaryTab
                    default: section(bodySections(current.note.body).notes, empty: "No notes.")
                    }
                    if mine {
                        Label("Recorded on this iPhone.", systemImage: "iphone").font(.footnote).foregroundStyle(Theme.sage)
                    } else {
                        Label("From the desktop. Read-only on this iPhone for now.", systemImage: "lock")
                            .font(.footnote).foregroundStyle(Theme.sage)
                    }
                }
                .padding(.horizontal, 22).padding(.vertical, 12)
            }
            .onChange(of: jump) { _, index in
                guard let index else { return }
                withAnimation { proxy.scrollTo(index, anchor: .center) }
                jump = nil
            }
        }
        .background(Theme.paper)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { asking = true } label: { Image(systemName: "sparkles") }
                    .accessibilityLabel("Ask about this meeting")
            }
            // A new name for the phone's own meetings (plan PL1); desktop meetings are read-only here
            if mine {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("Suggest a name", systemImage: "character.cursor.ibeam") { suggestName() }
                            .disabled(suggesting || AppleModel.unavailableReason != nil)
                        if vault.rootURL != nil {
                            // Into a folder (plan PN2)
                            Menu("Move to folder", systemImage: "folder") {
                                Button("Not in a folder") { move(to: nil) }.disabled(current.folderID == nil)
                                ForEach(vault.contents.folders, id: \.id) { folder in
                                    Button(folder.name) { move(to: folder.id) }.disabled(current.folderID == folder.id)
                                }
                            }
                        }
                    } label: {
                        Label("More", systemImage: "ellipsis.circle").labelStyle(.iconOnly)
                    }
                }
            }
        }
        .alert("Use this name?", isPresented: Binding(get: { suggested != nil }, set: { if !$0 { suggested = nil } })) {
            Button("Cancel", role: .cancel) {}
            Button("Use this name") {
                if let name = suggested {
                    let id = current.id
                    Task { await housekeeper.change(id, vault: vault) { setTitle(&$0, title: name, onlyIfDefault: false, now: Date(), deviceID: phoneDeviceID()) } }
                }
            }
        } message: {
            Text(suggested ?? "")
        }
        .sheet(isPresented: $asking) {
            NavigationStack {
                ScrollView {
                    AskPanel(
                        meetingID: current.id, lines: current.segments ?? [], speakers: speakers,
                        notes: bodySections(current.note.body).notes, startsOpen: true,
                        onJump: { seconds in
                            asking = false
                            tab = 0
                            jump = (current.segments ?? []).lastIndex { $0.start <= seconds + 0.5 }
                        },
                        onAdd: mine ? { turn in
                            Task {
                                await housekeeper.change(current.id, vault: vault) {
                                    addToNotes(&$0, question: turn.question, answer: turn.answer ?? "", now: Date(), deviceID: phoneDeviceID())
                                }
                            }
                        } : nil)
                        .padding(20)
                }
                .background(Theme.paper)
                .navigationTitle("Ask")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { asking = false } } }
            }
            .presentationDetents([.medium, .large])
        }
    }

    /// The summary, and for the phone's own meetings the controls to make one (PI19, PI20)
    @ViewBuilder private var summaryTab: some View {
        section(bodySections(current.note.body).summary, empty: isThoughts(current.note) ? "No prompt yet." : "No summary yet.")
        if let meta = current.note.summary, meta.status == "done", bodySections(current.note.body).summary != nil {
            // Only a summary this iPhone made says so; a desktop one names its model
            Text(mine && meta.model == AppleModel().name ? "AI generated on this iPhone." : "AI generated" + (meta.model.map { " by \($0)." } ?? "."))
                .font(.caption).foregroundStyle(Theme.inkMuted)
        }
        if mine {
            switch assistant.work[current.id] {
            case .summarizing?:
                HStack {
                    ProgressView()
                    Text(isThoughts(current.note) ? "Making the prompt on this iPhone" : "Making the summary on this iPhone").font(.footnote).foregroundStyle(Theme.inkMuted)
                    Spacer()
                    Button("Stop") { assistant.cancel(current.id) }
                }
            case .waitingToCool?:
                HStack {
                    Label(heatSummaryWait, systemImage: "thermometer.medium").font(.footnote).foregroundStyle(Theme.warning)
                    Spacer()
                    Button("Cancel") { assistant.cancel(current.id) }
                }
            case let .failed(message)?:
                Text(message).font(.footnote).foregroundStyle(Theme.warning)
                summarizeButton
            case nil:
                summarizeButton
            }
        }
    }

    private var summarizeButton: some View {
        let none = bodySections(current.note.body).summary == nil
        return Button(isThoughts(current.note) ? (none ? "Make it a prompt" : "Make the prompt again") : (none ? "Summarize" : "Summarize again")) {
            assistant.summarize(current.id, vault: vault, housekeeper: housekeeper)
        }
        .buttonStyle(.bordered)
        .disabled(current.segments?.isEmpty ?? true)
    }

    @ViewBuilder private var transcript: some View {
        if let segments = current.segments {
            if segments.isEmpty {
                Text("No transcript.").foregroundStyle(Theme.inkMuted)
            }
            ForEach(Array(segments.enumerated()), id: \.offset) { index, segment in
                // Recordings from this iPhone have no speakers yet
                let name = speakerName(segment.speaker)
                HStack(alignment: .top, spacing: 14) {
                    Circle().fill(Theme.speaker(segment.speaker)).frame(width: 34, height: 34)
                        .overlay(Text(String(name.prefix(1))).font(.subheadline.weight(.medium)).foregroundStyle(Theme.speakerInk(segment.speaker)))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 10) {
                            Text(name).font(.body.weight(.semibold)).foregroundStyle(Theme.ink)
                            Text(clock(segment.start)).font(.subheadline.monospacedDigit()).foregroundStyle(Theme.inkMuted)
                        }
                        Text(segment.text).foregroundStyle(Theme.ink).lineSpacing(3).textSelection(.enabled)
                    }
                }
                .padding(.vertical, 6)
                .id(index)
                .accessibilityElement(children: .combine)
            }
        } else {
            Text("The transcript is still syncing from iCloud.").foregroundStyle(Theme.inkMuted)
        }
    }

    private func speakerName(_ slot: Int64?) -> String {
        guard let speaker = current.note.speakers.first(where: { $0.slot == slot }) else { return "Speaker" }
        return vault.contents.person(speaker.person)?.name ?? speaker.name
    }

    @ViewBuilder private func section(_ text: String?, empty: String) -> some View {
        if let text {
            MarkdownBlocks(text: text)
        } else {
            Text(empty).foregroundStyle(Theme.inkMuted)
        }
    }
}

struct PeopleView: View {
    @Environment(VaultStore.self) private var vault
    var body: some View {
        List(vault.contents.people, id: \.id) { person in
            VStack(alignment: .leading, spacing: 4) {
                Text(person.name).foregroundStyle(Theme.ink)
                let count = vault.contents.meetings.filter { $0.note.speakers.contains { $0.person == person.id } }.count
                Text("\(count) meeting\(count == 1 ? "" : "s")").font(.footnote).foregroundStyle(Theme.inkMuted)
            }
            .listRowBackground(Theme.surface)
        }
        .scrollContentBackground(.hidden)
        .background(Theme.paper)
        .navigationTitle("People")
        .overlay {
            if vault.contents.people.isEmpty { ContentUnavailableView("No people yet", systemImage: "person.2") }
        }
    }
}

struct SettingsView: View {
    @Environment(VaultStore.self) private var vault
    @AppStorage("audioRetention") private var retention = Retention.standard.rawValue
    @AppStorage("summarizeWhenRecordingEnds") private var autoSummary = true
    @AppStorage(autoNameKey) private var autoName = true
    @AppStorage(titleFormatKey) private var nameFormat = defaultTitleFormat
    @Environment(\.dismiss) private var dismiss
    @State private var showDisclosure = false
    @State private var picking = false
    @State private var askCellular = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if let name = vault.folderName {
                        LabeledContent("Folder", value: name)
                        Button("Read again") { vault.reload() }
                        Button("Stop using this folder", role: .destructive) { vault.forget() }
                    } else {
                        Button("Choose the vault folder") { showDisclosure = true }
                    }
                    MoreAbout("Desktop meetings are read-only here; meetings you record on this iPhone are added to it. They show in the desktop app once it reads the vault (a later desktop update).")
                } header: {
                    Text("Vault")
                } footer: {
                    Text("The Quire folder in iCloud Drive that the desktop app writes.")
                }
                Section {
                    Picker("Keep audio", selection: $retention) {
                        ForEach(Retention.allCases, id: \.rawValue) { Text($0.label).tag($0.rawValue) }
                    }
                    MoreAbout("Changes apply to new recordings. Audio stays on this iPhone and never goes into the vault. Deleting removes it from this iPhone and from the vault folder, checked file by file. Deletion runs when Quire opens, when you return to it and when iOS allows a background check, so an iPhone that is not used cannot delete anything.")
                } header: {
                    Text("Audio retention")
                } footer: {
                    // What deletion cannot reach stays in view (Phase N review)
                    Text((Retention(rawValue: retention) ?? .standard).explanation
                        + " iCloud's Recently Deleted, backups and low-level storage recovery are outside Quire's control.")
                }
                Section("Transcription") { ModelRow(askCellular: $askCellular) }
                Section("Recording") {
                    NavigationLink("Telling people you're recording") { ConsentGuideView() }
                }
                Section {
                    Toggle("Summarize when a recording ends", isOn: $autoSummary).tint(Theme.accentStrong)
                } header: {
                    Text("Summaries and Ask")
                } footer: {
                    // A reason it cannot run is a warning, so it is never folded away
                    Text(AppleModel.unavailableReason ?? "Made by Apple's model on this iPhone; nothing is sent anywhere.")
                }
                Section {
                    Toggle("Name meetings automatically", isOn: $autoName).tint(Theme.accentStrong)
                    TextField("Name format", text: $nameFormat)
                        .autocorrectionDisabled().textInputAutocapitalization(.never)
                        .accessibilityLabel("Name format")
                    Text("Example: " + formatTitle(nameFormat, TitleFields(
                        start: Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: Date()),
                        topic: "Budget review", folder: "Garden Club")))
                        .font(.footnote).foregroundStyle(Theme.ink)
                    MoreAbout("In the format, {date} is the day (2026-11-03), {time} the start time (14.05), {topic} what the meeting was about and {folder} its folder. A name you typed is never replaced.")
                } header: {
                    Text("Meeting names")
                } footer: {
                    Text("Apple's model names a recording when it ends.")
                }
                Section("This version") {
                    Text("The app's only network request is the transcription model download you start.").foregroundStyle(Theme.inkMuted)
                }
            }
            .navigationTitle("Settings")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .alert("Use a vault folder?", isPresented: $showDisclosure) {
                Button("Cancel", role: .cancel) {}
                Button("Choose the folder") { picking = true }
            } message: {
                Text(vaultDisclosure)
            }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                if case let .success(url) = result { vault.open(url) }
            }
        }
    }
}
