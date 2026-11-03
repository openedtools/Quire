// Summaries and Ask in the app (plan I4, PI18 to PI21): a summary after each phone recording
// (unless turned off) and "Summarize again", written into the note through the I3 path; Ask on
// the recording screen and on every meeting, with citations to tap and "Add to note" for the
// phone's own meetings. Questions and answers stay in memory for this session only.

import Foundation
import Observation
import RecordingKit
import SwiftUI
import UIKit
import VaultKit

let autoSummaryKey = "summarizeWhenRecordingEnds"

/// Whether a summary is made when a recording ends (Settings, on unless turned off).
func autoSummaryOn() -> Bool { UserDefaults.standard.object(forKey: autoSummaryKey) as? Bool ?? true }

let autoNameKey = "nameMeetingsAutomatically"
let titleFormatKey = "meetingNameFormat"

/// Whether a recording is named when it ends (Settings, on unless turned off, plan PL1).
func autoNameOn() -> Bool { UserDefaults.standard.object(forKey: autoNameKey) as? Bool ?? true }

/// The name format from Settings (plan PL2).
func titleFormat() -> String {
    let saved = UserDefaults.standard.string(forKey: titleFormatKey)?.trimmingCharacters(in: .whitespaces) ?? ""
    return saved.isEmpty ? defaultTitleFormat : saved
}

/// A meeting read fresh from the phone's own folder or the vault.
func loadMeeting(_ id: String, vaultRoot: URL?) async -> VaultMeeting? {
    await Task.detached {
        if let local = try? VaultReader(root: localVaultFolder()).read().meetings.first(where: { $0.id == id }) { return local }
        guard let vaultRoot else { return nil }
        return try? VaultReader(root: vaultRoot).read().meetings.first { $0.id == id }
    }.value
}

@MainActor
@Observable
final class Assistant {
    enum Work: Equatable {
        case summarizing
        /// The phone is warm: the summary starts by itself once it cools (PI22)
        case waitingToCool
        case failed(String)
    }

    /// Summaries waiting for the phone to cool, started when it does
    private var waiting: [String: () -> Void] = [:]
    private var thermalObserver: NSObjectProtocol?

    init() {
        thermalObserver = NotificationCenter.default.addObserver(forName: ProcessInfo.thermalStateDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.startWaitingIfCool() }
        }
    }

    private func startWaitingIfCool() {
        guard heatAction(ProcessInfo.processInfo.thermalState) == .normal, !waiting.isEmpty else { return }
        let starts = waiting
        waiting = [:]
        starts.values.forEach { $0() }
    }

    /// Summaries being made or that failed, by meeting
    private(set) var work: [String: Work] = [:]
    private var tasks: [String: Task<Void, Never>] = [:]
    /// Ask history, by meeting, for this session only (F-QA-6)
    var turns: [String: [AskTurn]] = [:]
    /// The question being answered, by meeting; kept here so Stop survives leaving the screen
    var asking: [String: Task<Void, Never>] = [:]

    func summarize(_ id: String, vault: VaultStore, housekeeper: Housekeeper) {
        guard tasks[id] == nil else { return }
        if let reason = AppleModel.unavailableReason {
            work[id] = .failed(reason)
            return
        }
        // A warm phone makes no summary until it cools (PI22); it starts by itself then
        if heatAction(ProcessInfo.processInfo.thermalState) != .normal {
            work[id] = .waitingToCool
            waiting[id] = { [weak self] in self?.summarize(id, vault: vault, housekeeper: housekeeper) }
            return
        }
        work[id] = .summarizing
        // A summary can take a minute; keep going briefly if the app goes to the background, and
        // stop cleanly when iOS ends that time (Summarize again picks it up)
        var background = UIBackgroundTaskIdentifier.invalid
        background = UIApplication.shared.beginBackgroundTask(withName: "Summary") { [weak self] in
            Task { @MainActor in
                self?.tasks[id]?.cancel()
                UIApplication.shared.endBackgroundTask(background)
                background = .invalid
            }
        }
        tasks[id] = Task {
            defer {
                tasks[id] = nil
                if background != .invalid { UIApplication.shared.endBackgroundTask(background) }
                background = .invalid
            }
            guard let meeting = await loadMeeting(id, vaultRoot: vault.rootURL) else {
                work[id] = .failed("The meeting could not be read.")
                return
            }
            do {
                let speakers = Dictionary(meeting.note.speakers.map { ($0.slot, $0.name) }, uniquingKeysWith: { a, _ in a })
                // Thoughts become one clear prompt instead of meeting notes (PN4)
                let markdown = isThoughts(meeting.note)
                    ? try await makePrompt(segments: meeting.segments ?? [], model: AppleModel())
                    : try await Summarizer(model: AppleModel()).summarize(
                        segments: meeting.segments ?? [], speakers: speakers, notes: bodySections(meeting.note.body).notes)
                try Task.checkCancellation()
                let model = AppleModel().name
                await housekeeper.change(id, vault: vault) { setSummary(&$0, markdown: markdown, model: model, now: Date(), deviceID: phoneDeviceID()) }
                work[id] = nil
            } catch {
                work[id] = error is CancellationError ? nil : .failed(assistProblem(error))
            }
        }
    }

    /// A name for the meeting from Apple's model, in the format from Settings (plan PL1).
    func suggestName(_ meeting: VaultMeeting, folder: String) async throws -> String? {
        if let reason = AppleModel.unavailableReason { throw TextModelError.unavailable(reason) }
        let note = meeting.note
        let speakers = Dictionary(note.speakers.map { ($0.slot, $0.name) }, uniquingKeysWith: { a, _ in a })
        let start = note.started.flatMap(parseTimestamp) ?? parseTimestamp(note.created) ?? Date()
        return try await suggestTitle(segments: meeting.segments ?? [], speakers: speakers, notes: bodySections(note.body).notes,
                                      start: start, folder: folder, format: titleFormat(), model: AppleModel())
    }

    /// After a recording: names it if it still has its default name, quietly. A warm phone or a
    /// missing model leaves the default name.
    func nameAfterRecording(_ id: String, vault: VaultStore, housekeeper: Housekeeper) async {
        guard autoNameOn(), AppleModel.unavailableReason == nil, heatAction(ProcessInfo.processInfo.thermalState) == .normal,
              let meeting = await loadMeeting(id, vaultRoot: vault.rootURL), isDefaultTitle(meeting.note.title) else { return }
        let folder = vault.contents.folders.first { $0.id == meeting.folderID }?.name ?? ""
        guard let name = try? await suggestName(meeting, folder: folder) else { return }
        await housekeeper.change(id, vault: vault) { setTitle(&$0, title: name, onlyIfDefault: true, now: Date(), deviceID: phoneDeviceID()) }
    }

    func cancel(_ id: String) {
        tasks[id]?.cancel()
        if waiting.removeValue(forKey: id) != nil { work[id] = nil }
    }
}

struct AskTurn: Identifiable, Equatable {
    let id = UUID()
    let question: String
    var answer: String?
    var citations: [AskCitation] = []
    var answeredFrom = ""
    var problem: String?
}

/// The Ask box, collapsed until opened (desktop F-QA-1). `lines` is the transcript so far.
struct AskPanel: View {
    @Environment(Assistant.self) private var assistant
    let meetingID: String
    let lines: [Segment]
    var speakers: [Int64: String] = [:]
    var notes: String?
    /// Seconds transcription is behind, during a recording
    var backlog: Double = 0
    /// Transcription is held because the phone is warm
    var heatHeld = false
    /// Opened already (the meeting's Ask sheet); collapsed on the recording screen
    var startsOpen = false
    var onJump: (Double) -> Void
    /// "Add to note", for the phone's own meetings
    var onAdd: ((AskTurn) -> Void)?

    @State private var open = false
    @State private var question = ""
    @State private var added: Set<UUID> = []
    @FocusState private var typing: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        // In the meeting's Ask sheet the panel is simply there; on the recording screen it folds away
        if startsOpen {
            panel
        } else {
            DisclosureGroup(isExpanded: $open) { panel } label: {
                Label("Ask about this meeting", systemImage: "sparkles").foregroundStyle(Theme.ink)
            }
            .tint(Theme.ink)
        }
    }

    private var panel: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let reason = AppleModel.unavailableReason {
                Text(reason).font(.footnote).foregroundStyle(Theme.inkMuted)
            }
            ForEach(assistant.turns[meetingID] ?? []) { turn in turnView(turn) }
            // Stacked at the accessibility text sizes, so the question has the full width
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading) { questionField; askButton }
            } else {
                HStack { questionField; askButton }
            }
            Text("Answers come from the transcript on this iPhone and are AI generated; check them against the lines they cite.")
                .font(.caption).foregroundStyle(Theme.inkMuted)
        }
        .padding(.top, 8)
    }

    private var questionField: some View {
        // One line, so Return asks the question (a multi-line field would add a line break)
        TextField("Question about this meeting", text: $question)
            .textFieldStyle(.roundedBorder)
            .submitLabel(.send)
            .accessibilityLabel("Question about this meeting")
            .focused($typing)
            .onSubmit(ask)
    }

    @ViewBuilder private var askButton: some View {
        if let running = assistant.asking[meetingID] {
            Button("Stop") { running.cancel() }
        } else {
            Button("Ask", action: ask)
                .disabled(question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || AppleModel.unavailableReason != nil)
        }
    }

    @ViewBuilder private func turnView(_ turn: AskTurn) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(turn.question).font(.subheadline.weight(.semibold)).foregroundStyle(Theme.ink)
            if let answer = turn.answer {
                MarkdownBlocks(text: answer)
                if !turn.citations.isEmpty {
                    HStack {
                        ForEach(turn.citations, id: \.label) { c in
                            Button(c.label) { onJump(c.seconds) }
                                .font(.caption.monospacedDigit()).buttonStyle(.bordered)
                                .accessibilityLabel("Go to \(c.label)")
                        }
                    }
                }
                HStack {
                    Text(["AI generated", turn.answeredFrom].filter { !$0.isEmpty }.joined(separator: ". "))
                        .font(.caption).foregroundStyle(Theme.inkMuted)
                    Spacer()
                    CopyButton(text: answer, what: "answer")
                    if let onAdd, !added.contains(turn.id) {
                        Button("Add to note") {
                            added.insert(turn.id)
                            onAdd(turn)
                        }
                        .font(.caption)
                    }
                }
            } else if let problem = turn.problem {
                Text(problem).font(.footnote).foregroundStyle(Theme.warning)
            } else {
                ProgressView().accessibilityLabel("Answering")
            }
        }
        .padding(12)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12))
    }

    private func ask() {
        let q = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty, assistant.asking[meetingID] == nil else { return }
        guard q.count <= 2000 else {
            append(AskTurn(question: String(q.prefix(80)), problem: "Questions can be up to 2000 characters."))
            return
        }
        question = ""
        typing = false
        var turn = AskTurn(question: q)
        // A recording is never slowed by Ask (desktop F-QA-5), nor a warm phone warmed further
        if heatHeld {
            turn.problem = "This iPhone is warm. Ask again when it has cooled and the transcript has caught up."
            append(turn)
            return
        }
        if backlog > 10 {
            turn.problem = "Transcription is \(Int(backlog.rounded())) seconds behind. Ask again when it catches up."
            append(turn)
            return
        }
        append(turn)
        let lines = lines, speakers = speakers, notes = notes, id = turn.id, meeting = meetingID
        let assistant = assistant
        assistant.asking[meeting] = Task {
            defer { assistant.asking[meeting] = nil }
            do {
                let model = AppleModel()
                var budgets = askBudgets(window: await model.contextTokens)
                var context = buildAskContext(segments: lines, speakers: speakers, notes: notes, question: q, budgets: budgets)
                guard !context.text.isEmpty else { throw TextModelError.nothingToSummarize }
                let raw: String
                do {
                    raw = try await model.respond(instructions: askInstructions, prompt: askPrompt(context: context.text, question: q))
                } catch TextModelError.tooLong {
                    // The token count is an estimate: once more with half the transcript
                    budgets = AskBudgets(recentTokens: budgets.recentTokens / 2, retrievalTokens: budgets.retrievalTokens / 2)
                    context = buildAskContext(segments: lines, speakers: speakers, notes: notes, question: q, budgets: budgets)
                    raw = try await model.respond(instructions: askInstructions, prompt: askPrompt(context: context.text, question: q))
                }
                try Task.checkCancellation()
                let (answer, citations) = checkCitations(raw, spans: context.spans)
                update(assistant, meeting, id) {
                    $0.answer = answer
                    $0.citations = citations
                    $0.answeredFrom = answeredFrom(context, meetingEnd: lines.map(\.end).max() ?? 0)
                }
            } catch {
                update(assistant, meeting, id) { $0.problem = assistProblem(error) }
            }
        }
    }

    private func append(_ turn: AskTurn) {
        assistant.turns[meetingID, default: []].append(turn)
    }

    private func update(_ assistant: Assistant, _ meeting: String, _ id: UUID, _ change: (inout AskTurn) -> Void) {
        guard var list = assistant.turns[meeting], let i = list.firstIndex(where: { $0.id == id }) else { return }
        change(&list[i])
        assistant.turns[meeting] = list
    }
}
