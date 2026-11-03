// Thought mode (plan PN4): the shared instructions and name, the note's kind, and the prompt made
// in one call or, for a long talk, from tidied parts. Every word here is made up.

import Foundation
import Testing
@testable import RecordingKit
import VaultKit

private func fixture() throws -> [String: Any] {
    let url = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("docs/vault-fixtures/thoughts.json")
    return try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
}

@Test func theInstructionsAndNameAreTheDesktops() throws {
    let f = try fixture()
    #expect(f["instructions"] as? String == thoughtInstructions)
    #expect(f["folder"] as? String == thoughtsFolderName)
    #expect(f["kindValue"] as? String == thoughtsKind)
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC")!
    let date = try #require(calendar.date(from: DateComponents(year: 2026, month: 11, day: 3, hour: 14, minute: 5)))
    #expect(defaultThoughtsTitle(date, calendar: calendar) == "Thoughts, 3 Nov 2026 14:05")
    let titles = try #require(f["defaultTitles"] as? [String: [String]])
    for t in titles["yes"]! { #expect(isDefaultTitle(t), "\(t)") }
    for t in titles["no"]! { #expect(!isDefaultTitle(t), "\(t)") }
}

@Test func aThoughtsNoteCarriesItsKindThroughTheVaultFormat() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("thoughts-\(UUID().uuidString)")
    var session = RecordingSession(folder: dir, title: "Thoughts, 3 Nov 2026 14:05", started: Date(timeIntervalSince1970: 1_790_000_000), deviceID: "dev-0a1b2c3d", transcriptOnly: true, kind: thoughtsKind)
    try session.add(text: "An app for the garden club rota.", start: 0, end: 3)
    let note = try parseNote(try Data(contentsOf: session.noteURL))
    #expect(isThoughts(note))
    #expect(renderNote(note).contains("\nkind: \"thoughts\"\n"))
    let plain = RecordingSession(folder: dir, title: "Bench vote", started: Date(), deviceID: "dev-0a1b2c3d", transcriptOnly: true)
    #expect(!isThoughts(plain.note(sidecar: nil)))
}

@Test func aShortTalkIsOnePromptCall() async throws {
    let model = StandInModel(window: 4096) { instructions, prompt in
        #expect(instructions == thoughtInstructions)
        #expect(prompt.contains("<transcript>\nI want an app\nfor the garden rota\n</transcript>"))
        return "Build an app for the garden club rota."
    }
    let segments = [
        Segment(id: "b", start: 2, end: 3, speaker: nil, text: "for the garden rota"),
        Segment(id: "a", start: 0, end: 1, speaker: nil, text: " I want an app "),
    ]
    #expect(try await makePrompt(segments: segments, model: model) == "Build an app for the garden club rota.")
    #expect(model.prompts.count == 1)
}

@Test func aLongTalkIsTidiedInPartsThenMadeIntoOnePrompt() async throws {
    let model = StandInModel(window: 4096) { instructions, _ in
        instructions == thoughtInstructions ? "The final prompt." : "- A tidied part."
    }
    let segments = (0..<600).map { Segment(id: "s\($0)", start: Double($0), end: Double($0) + 1, speaker: nil, text: "Another thought about the garden club rota and who waters the beds on Sundays.") }
    #expect(try await makePrompt(segments: segments, model: model) == "The final prompt.")
    let parts = model.prompts.filter { $0.instructions == thoughtPartInstructions }.count
    #expect(parts > 1 && model.prompts.count == parts + 1)
    await #expect(throws: TextModelError.nothingToSummarize) { try await makePrompt(segments: [], model: model) }
}
