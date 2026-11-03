// The phone's heat during a recording (plan PI22, NF-IOS-1): at "serious" transcription and
// summaries wait while the audio keeps recording; at "critical" the recording stops and saves.

import Foundation

public enum HeatAction: Equatable, Sendable {
    case normal
    /// Keep recording; hold transcription and summaries until the phone cools
    case hold
    /// Stop and save, as for a phone call
    case stop
}

public func heatAction(_ state: ProcessInfo.ThermalState) -> HeatAction {
    switch state {
    case .serious: .hold
    case .critical: .stop
    default: .normal
    }
}

public let heatHoldNote = "This iPhone is warm, so Quire keeps recording and the transcript catches up when it cools."
public let heatStopNote = "The recording stopped because this iPhone got too hot. What was recorded is saved."
public let heatSummaryWait = "This iPhone is warm. The summary starts by itself when it cools."
public let heatFinishNote = "This iPhone is too hot to finish the transcript now. It continues when the phone cools; the audio is saved."
public let heatLongNote = "The recording stopped because this iPhone stayed hot for half an hour. What was recorded is saved, and the transcript catches up when it cools."
/// The longest stretch of speech held in memory while the phone is warm, before stopping
public let heatHoldLimitSeconds: Double = 1800

/// A piece of speech kept as 16-bit samples while it waits: half the memory of the float samples.
public struct HeldPiece: Sendable {
    public let start: Double
    public let pcm: [Int16]
    public var end: Double { start + Double(pcm.count) / Double(PauseSegmenter.sampleRate) }

    public init(_ piece: AudioPiece) {
        start = piece.start
        pcm = piece.samples.map { Int16(max(-1, min(1, $0)) * 32_767) }
    }

    public var samples: [Float] { pcm.map { Float($0) / 32_767 } }
}
