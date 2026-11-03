// Apple's on-device model for summaries and Ask (plan PI4, PI18): nothing to download and
// nothing sent anywhere. Its window is read at run time, and when it is not available the app
// says why in plain words instead of failing.

import Foundation
import FoundationModels
import RecordingKit

struct AppleModel: TextModel {
    let name = "apple-on-device"

    var contextTokens: Int {
        get async { SystemLanguageModel.default.contextSize }
    }

    /// Why summaries and Ask cannot run on this iPhone now, or nil when they can.
    static var unavailableReason: String? {
        switch SystemLanguageModel.default.availability {
        case .available: nil
        case .unavailable(.deviceNotEligible): "This iPhone does not support Apple Intelligence, so summaries and Ask are not available on it."
        case .unavailable(.appleIntelligenceNotEnabled): "Turn on Apple Intelligence in Settings to make summaries and ask questions."
        case .unavailable(.modelNotReady): "Apple Intelligence is still getting ready on this iPhone. Try again in a little while."
        case .unavailable: "Summaries and Ask are not available on this iPhone right now."
        }
    }

    func respond(instructions: String, prompt: String) async throws -> String {
        if let reason = Self.unavailableReason { throw TextModelError.unavailable(reason) }
        // A fresh session for every call: no history carries over between meetings or questions
        let session = LanguageModelSession(instructions: instructions)
        do {
            return try await session.respond(to: prompt).content
        } catch let error as LanguageModelSession.GenerationError {
            switch error {
            case .exceededContextWindowSize: throw TextModelError.tooLong
            case .unsupportedLanguageOrLocale: throw TextModelError.unsupportedLanguage
            case .guardrailViolation, .refusal: throw TextModelError.refused
            case .assetsUnavailable: throw TextModelError.unavailable("Apple Intelligence is still getting ready on this iPhone. Try again in a little while.")
            default: throw error
            }
        }
    }
}

/// What a failed summary or answer says, in plain words.
func assistProblem(_ error: Error) -> String {
    switch error as? TextModelError {
    case let .unavailable(reason)?: reason
    case .unsupportedLanguage?: "Summaries and Ask are not available in this meeting's language yet."
    case .refused?: "The summary could not be made: Apple's model declined this content. The transcript is unaffected."
    case .nothingToSummarize?: "There is no transcript to work from yet."
    case .tooLong?, .emptyAnswer?: "The model could not finish. Try again."
    case nil: error is CancellationError ? "Stopped." : "The model could not finish. Try again."
    }
}
