// The audio file a recording writes (plan PI9, PI23): AAC, mono, 16 kHz, about 22 MB an hour, in
// ADTS framing (`.aac`), where every frame carries its own header, so a crash or a killed app
// leaves a file that plays up to the last second written. `.m4a` and `.caf` are unreadable until
// closed. Audio stays on the phone (it never enters the vault in version 1).

import AVFoundation

public enum RecordingAudio {
    public static let fileExtension = "aac"

    public static var settings: [String: Any] { [
        AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 16_000, AVNumberOfChannelsKey: 1, AVEncoderBitRateKey: 48_000,
    ] }

    public static func makeFile(at url: URL) throws -> AVAudioFile {
        try AVAudioFile(forWriting: url, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
    }
}
