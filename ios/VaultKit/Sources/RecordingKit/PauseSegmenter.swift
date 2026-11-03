// Cuts 16 kHz mono audio into pieces at pauses, so each piece can be transcribed as soon as the
// speaker stops (plan PI7). Loudness against an adaptive noise floor: Parakeet copes with
// silence itself, so the cut only has to land between words, not be exact.

public struct AudioPiece: Equatable, Sendable {
    /// Seconds from the start of the recording
    public let start: Double
    public let samples: [Float]
    public var end: Double { start + Double(samples.count) / Double(PauseSegmenter.sampleRate) }

    public init(start: Double, samples: [Float]) {
        self.start = start
        self.samples = samples
    }
}

public struct PauseSegmenter: Sendable {
    public static let sampleRate = 16_000
    /// 30 ms frames
    static let frame = 480
    /// Silence this long after speech ends a piece
    public var pauseSeconds = 0.6
    /// Pieces shorter than this wait for more speech (unless the recording ends)
    public var minimumSeconds = 1.0
    /// A piece never grows past this, so a long monologue still shows lines
    public var maximumSeconds = 14.0

    private var buffer: [Float] = []
    private var bufferStart = 0
    private var consumed = 0
    private var noiseFloor: Float = 0.002
    private var silentFrames = 0
    private var heardSpeech = false
    private var pending: [Float] = []

    public init() {}

    private static func rms(_ frame: ArraySlice<Float>) -> Float {
        var sum: Float = 0
        for s in frame { sum += s * s }
        return (sum / Float(max(frame.count, 1))).squareRoot()
    }

    /// Adds audio; returns any pieces that are now complete.
    public mutating func push(_ samples: [Float]) -> [AudioPiece] {
        pending += samples
        var out: [AudioPiece] = []
        let frame = Self.frame
        // A read position rather than removing from the front, so an hour-long import stays linear
        var at = 0
        defer { pending.removeFirst(at) }
        while pending.count - at >= frame {
            let chunk = pending[at..<(at + frame)]
            at += frame
            let level = Self.rms(chunk)
            let speech = level > max(noiseFloor * 3, 0.004)
            if speech {
                heardSpeech = true
                silentFrames = 0
            } else {
                silentFrames += 1
                // The floor follows the room slowly, only from quiet frames
                noiseFloor = noiseFloor * 0.95 + level * 0.05
            }
            buffer += chunk
            consumed += frame
            let seconds = Double(buffer.count) / Double(Self.sampleRate)
            let paused = Double(silentFrames * frame) / Double(Self.sampleRate) >= pauseSeconds
            if heardSpeech && ((paused && seconds >= minimumSeconds) || seconds >= maximumSeconds) {
                out.append(cut())
            } else if !heardSpeech && seconds >= maximumSeconds {
                // Nothing but quiet: drop it, it holds no words
                bufferStart = consumed
                buffer = []
            }
        }
        return out
    }

    private mutating func cut() -> AudioPiece {
        let piece = AudioPiece(start: Double(bufferStart) / Double(Self.sampleRate), samples: buffer)
        bufferStart = consumed
        buffer = []
        heardSpeech = false
        silentFrames = 0
        return piece
    }

    /// The rest, when the recording stops or pauses.
    public mutating func flush() -> AudioPiece? {
        buffer += pending
        consumed += pending.count
        pending = []
        guard heardSpeech, !buffer.isEmpty else {
            bufferStart = consumed
            buffer = []
            return nil
        }
        return cut()
    }
}
