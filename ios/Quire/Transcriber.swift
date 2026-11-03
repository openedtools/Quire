// Parakeet v3 on the Neural Engine through FluidAudio, loaded from the checked local files only
// (no download of its own). One piece of audio at a time (plan PI7).

import AVFoundation
import FluidAudio
import Foundation
import RecordingKit

actor Transcriber {
    private var manager: AsrManager?

    func load(from directory: URL) throws {
        guard manager == nil else { return }
        let models = try AsrModels.loadLocal(from: directory, version: .v3)
        manager = AsrManager(config: .default, models: models)
    }

    /// The text of one piece. Very short pieces are padded with silence to a full second.
    func transcribe(_ samples: [Float]) async throws -> String {
        guard let manager else { return "" }
        var audio = samples
        if audio.count < PauseSegmenter.sampleRate {
            audio += [Float](repeating: 0, count: PauseSegmenter.sampleRate - audio.count)
        }
        var state = TdtDecoderState.make()
        return try await manager.transcribe(audio, decoderState: &state).text
    }
}

/// Reads any audio file iOS can decode as 16 kHz mono samples (for import, F-IOS-REC-4).
func readSamples(from url: URL) throws -> [Float] {
    let file = try AVAudioFile(forReading: url)
    let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: Double(PauseSegmenter.sampleRate), channels: 1, interleaved: false)!
    guard let converter = AVAudioConverter(from: file.processingFormat, to: target) else { throw URLError(.cannotDecodeContentData) }
    var out: [Float] = []
    let inputCapacity: AVAudioFrameCount = 16_384
    var finished = false
    while !finished {
        let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: 16_384)!
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, inputStatus in
            let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: inputCapacity)!
            do {
                try file.read(into: input, frameCount: inputCapacity)
            } catch {
                inputStatus.pointee = .endOfStream
                return nil
            }
            if input.frameLength == 0 {
                inputStatus.pointee = .endOfStream
                return nil
            }
            inputStatus.pointee = .haveData
            return input
        }
        if let error { throw error }
        if let data = output.floatChannelData, output.frameLength > 0 {
            out += Array(UnsafeBufferPointer(start: data[0], count: Int(output.frameLength)))
        }
        finished = status == .endOfStream || status == .error || output.frameLength == 0
    }
    return out
}
