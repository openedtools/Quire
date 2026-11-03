// I5: heat (PI22) and audio that survives a crash (PI23).

import AVFoundation
import Foundation
import Testing
@testable import RecordingKit

@Test func heatHoldsAtSeriousAndStopsAtCritical() {
    #expect(heatAction(.nominal) == .normal)
    #expect(heatAction(.fair) == .normal)
    #expect(heatAction(.serious) == .hold)
    #expect(heatAction(.critical) == .stop)
}

@Test func aRecordingCutOffMidWayStillPlaysToTheLastSecond() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("cut-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let url = dir.appendingPathComponent("rec.\(RecordingAudio.fileExtension)")
    let file = try RecordingAudio.makeFile(at: url)
    let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    for second in 0..<20 {
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 16_000)!
        buffer.frameLength = 16_000
        for i in 0..<16_000 { buffer.floatChannelData![0][i] = 0.2 * sin(Float(second * 16_000 + i) * 0.1) }
        try file.write(from: buffer)
    }
    // The app is killed here: the bytes on disk, with the file never closed
    let copy = dir.appendingPathComponent("copy.\(RecordingAudio.fileExtension)")
    try FileManager.default.copyItem(at: url, to: copy)
    let read = try AVAudioFile(forReading: copy)
    #expect(Double(read.length) / read.fileFormat.sampleRate > 19, "all but the last frame or two")
    _ = file
}

@Test func theSweeperCleansUpTheRecordingFormatOnThePhoneOnly() {
    #expect(AudioSweeper.localExtensions.contains("aac"))
    #expect(!AudioSweeper.extensions.contains("aac"), "vault audio keeps the format's list (section 10)")
}
