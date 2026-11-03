// AC-IOS-REC-3: Parakeet v3 transcribes on the device from the checked local files, and an
// imported recording becomes a valid meeting. The clip is a fictional sentence made with `say`.
// Needs the model files: `TEST_RUNNER_QUIRE_MODEL_DIR=<folder> xcodebuild test ...`.

import Foundation
import RecordingKit
import Testing
import VaultKit
@testable import Quire

private var modelDirectory: URL? {
    ProcessInfo.processInfo.environment["QUIRE_MODEL_DIR"].map { URL(fileURLWithPath: $0) }
}

private var clip: URL {
    URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Fixtures/garden-benches.m4a")
}

@Test(.enabled(if: modelDirectory != nil, "set TEST_RUNNER_QUIRE_MODEL_DIR to the model folder"))
func aSpokenSentenceComesBackAsText() async throws {
    let transcriber = Transcriber()
    try await transcriber.load(from: modelDirectory!)
    let samples = try readSamples(from: clip)
    #expect(samples.count > 16_000 * 3, "the clip decodes to 16 kHz samples")
    let text = try await transcriber.transcribe(samples).lowercased()
    #expect(text.contains("garden club"))
    #expect(text.contains("benches"))
}

@Test(.enabled(if: modelDirectory != nil, "set TEST_RUNNER_QUIRE_MODEL_DIR to the model folder"))
@MainActor func anImportedRecordingBecomesAMeeting() async throws {
    let id = try await Recorder.importRecording(from: clip, transcriptOnly: true, transcriber: Transcriber(), modelDirectory: modelDirectory!)
    let folder = localVaultFolder()
    let names = try FileManager.default.contentsOfDirectory(atPath: folder.path)
    let notes = names.filter { $0.hasSuffix(".md") }.compactMap { try? parseNote(try Data(contentsOf: folder.appendingPathComponent($0))) }
    let note = try #require(notes.first { $0.id == id })
    #expect(note.mode == "transcript_only" && note.audio.state == "none")
    #expect(note.source == "imported" && note.started == nil && note.durationS == nil, "an import has no recording times")
    #expect(!names.contains("\(id).m4a"), "transcript only keeps no audio")
    let ref = try #require(note.transcript)
    let (_, segments) = try parseSidecar(try Data(contentsOf: folder.appendingPathComponent(ref.file)))
    #expect(segments.map(\.text).joined(separator: " ").lowercased().contains("benches"))
}

// ===== I4.2: Apple's on-device model (runs only where Apple Intelligence is on) =====

@Test func theModelSaysWhyWhenItCannotRun() {
    // Whatever this machine's state, the app has a plain answer and never a crash
    if let reason = AppleModel.unavailableReason {
        #expect(!reason.isEmpty)
    }
}

@Test(.enabled(if: AppleModel.unavailableReason == nil, "Apple Intelligence is not on here"))
func aRealSummaryComesBackFromTheOnDeviceModel() async throws {
    let model = AppleModel()
    #expect(await model.contextTokens >= 2048, "the window is read at run time")
    let segments = [
        Segment(id: "seg-1", start: 2, end: 6, speaker: nil, text: "We agreed to buy two benches for the pond."),
        Segment(id: "seg-2", start: 7, end: 11, speaker: nil, text: "Sam will order them by Friday."),
    ]
    let summary = try await Summarizer(model: model).summarize(segments: segments, notes: nil)
    #expect(summary.lowercased().contains("bench"))
}

// ===== AC-IOS-3: no network request during a summary or Ask =====

/// Counts every request made through the URL loading system while registered
final class RequestCounter: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var count = 0
    override class func canInit(with request: URLRequest) -> Bool {
        count += 1
        return false
    }
}

@Test(.enabled(if: AppleModel.unavailableReason == nil, "Apple Intelligence is not on here"))
func aSummaryAndAnAnswerMakeNoNetworkRequest() async throws {
    URLProtocol.registerClass(RequestCounter.self)
    defer { URLProtocol.unregisterClass(RequestCounter.self) }
    RequestCounter.count = 0
    let model = AppleModel()
    let segments = [Segment(id: "seg-1", start: 20, end: 30, speaker: nil, text: "The water tap costs forty pounds to fix.")]
    _ = try await Summarizer(model: model).summarize(segments: segments, notes: nil)
    let context = buildAskContext(segments: segments, speakers: [:], notes: nil, question: "What does the tap cost?", budgets: askBudgets(window: await model.contextTokens))
    let answer = try await model.respond(instructions: askInstructions, prompt: askPrompt(context: context.text, question: "What does the tap cost?"))
    #expect(!answer.isEmpty)
    #expect(RequestCounter.count == 0)
}
