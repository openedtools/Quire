// Recording from the microphone with a live transcript (spec F-IOS-REC-1, 2, 5): the audio goes
// to an AAC file unless transcript only is on, and to the pause detector, whose pieces are
// transcribed one by one and saved as they arrive. Keeps going with the screen locked (the
// background audio mode). Also imports a recording (F-IOS-REC-4).

@preconcurrency import AVFoundation
import Foundation
import Observation
import RecordingKit
import UIKit
import VaultKit

/// This phone's device ID in the vault (`dev-` plus 8 hex characters), made once.
func phoneDeviceID() -> String {
    #if DEBUG
    // UI tests fix it, to place a meeting this iPhone recorded in a test vault (debug builds only)
    if let id = ProcessInfo.processInfo.environment["QUIRE_DEVICE_ID"], id.wholeMatch(of: /dev-[0-9a-f]{8}/) != nil { return id }
    #endif
    let key = "vaultDeviceID"
    if let id = UserDefaults.standard.string(forKey: key) { return id }
    let id = String(format: "dev-%08x", UInt32.random(in: 0...UInt32.max))
    UserDefaults.standard.set(id, forKey: key)
    return id
}

/// Recordings made on this phone, in the vault format, in the app's own storage (plan PI10).
func localVaultFolder() -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("OnThisPhone")
    let marker = base.appendingPathComponent("quire-format.json")
    if !FileManager.default.fileExists(atPath: marker.path) {
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        let m = Marker(format: "quire-vault", version: 1, minReader: 1, vaultID: newULID(at: Date()), created: timestampString(Date()), createdBy: phoneDeviceID())
        try? Data(renderMarker(m).utf8).write(to: marker, options: .atomic)
    }
    return base
}

/// Converts microphone buffers to 16 kHz mono on the audio thread (the tap's own thread).
final class TapConverter: @unchecked Sendable {
    private let converter: AVAudioConverter
    let format: AVAudioFormat

    init?(from input: AVAudioFormat, to format: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: input, to: format) else { return nil }
        self.converter = converter
        self.format = format
    }

    func convert(_ buffer: AVAudioPCMBuffer) -> Converted? {
        let ratio = format.sampleRate / buffer.format.sampleRate
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 64) else { return nil }
        // The converter calls back on this thread, before convert returns
        nonisolated(unsafe) var given = false
        nonisolated(unsafe) let buffer = buffer
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if given {
                status.pointee = .noDataNow
                return nil
            }
            given = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let data = out.floatChannelData, out.frameLength > 0 else { return nil }
        return Converted(buffer: out, samples: Array(UnsafeBufferPointer(start: data[0], count: Int(out.frameLength))))
    }
}

/// A converted buffer handed from the audio thread to the main actor once, never shared after.
struct Converted: @unchecked Sendable {
    let buffer: AVAudioPCMBuffer
    let samples: [Float]
}

@MainActor
@Observable
final class Recorder {
    enum Phase: Equatable {
        case idle
        case recording
        case paused
        case finishing
        case done
        case failed(String)
    }

    private(set) var phase: Phase = .idle
    private(set) var lines: [Segment] = []
    private(set) var level: Float = 0
    private(set) var elapsed: Double = 0
    private(set) var meetingID: String?
    /// The phone's heat as it affects this recording (PI22)
    private(set) var heat: HeatAction = .normal
    /// Transcription waits because the phone is warm
    var heatHeld: Bool { heat != .normal }
    /// What the heat means for this recording, shown on its own line beside other notes
    private(set) var heatNote: String?
    /// Seconds of speech waiting to be transcribed, the piece being transcribed included (Ask
    /// waits while this is over 10 s)
    var backlogSeconds: Double { queue.reduce(workingSeconds) { $0 + $1.end - $1.start } }
    /// Minutes of speech still to transcribe, shown while finishing waits for the phone to cool
    var minutesLeft: Int { Int((backlogSeconds / 60).rounded(.up)) }
    private var workingSeconds: Double = 0
    /// The notes so far (bookmarks), for Ask during the recording
    var notesText: String? { session.flatMap { bodySections($0.body).notes } }
    /// Why the recording stopped on its own, or a problem saving or transcribing, said plainly
    private(set) var interruptionNote: String?

    private let transcriber: Transcriber?
    private let transcribe: @Sendable ([Float]) async throws -> String
    private var session: RecordingSession?
    private var segmenter = PauseSegmenter()
    private let engine = AVAudioEngine()
    private var file: AVAudioFile?
    private var queue: [HeldPiece] = []
    private var tapInstalled = false
    private var thermalObserver: NSObjectProtocol?
    private var working = false
    private var startedAt = Date()
    private var pausedTotal: Double = 0
    private var pausedAt: Date?
    private var clockTask: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private var cancelled = false
    private let format16k = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

    init(transcriber: Transcriber) {
        self.transcriber = transcriber
        transcribe = { try await transcriber.transcribe($0) }
    }

    #if DEBUG
    /// Tests: a recorder with a stand-in transcriber, already "recording" into `folder`, no microphone
    init(testTranscribe: @escaping @Sendable ([Float]) async throws -> String, folder: URL) {
        transcriber = nil
        transcribe = testTranscribe
        session = RecordingSession(folder: folder, title: "Test", started: Date(), deviceID: "dev-00000000", transcriptOnly: true)
        phase = .recording
    }

    /// Tests: pieces of speech arriving, as from the microphone
    func testArrive(_ pieces: [AudioPiece]) {
        queue += pieces.map(HeldPiece.init)
        afterArrival()
    }
    #endif

    /// Recorded time so far, without paused time (the same clock as the audio file and the lines)
    private var recordedSeconds: Double {
        let pausedNow = pausedAt.map { Date().timeIntervalSince($0) } ?? 0
        return max(0, Date().timeIntervalSince(startedAt) - pausedTotal - pausedNow)
    }

    /// The screen closed before the recording got going: start stops at its next step and leaves nothing.
    func cancel() {
        cancelled = true
    }

    func start(title: String, transcriptOnly: Bool, modelDirectory: URL, retention: Retention, folderID: String?, kind: String? = nil) async {
        guard phase == .idle else { return }
        do {
            guard await AVAudioApplication.requestRecordPermission() else {
                phase = .failed("Quire needs the microphone to record. Allow it in Settings, Privacy, Microphone.")
                return
            }
            guard !cancelled else { return }
            try await transcriber?.load(from: modelDirectory)
            guard !cancelled else { return }
            let audioSession = AVAudioSession.sharedInstance()
            try audioSession.setCategory(.record, mode: .default, options: [.allowBluetoothHFP])
            try audioSession.setActive(true)

            startedAt = Date()
            let s = RecordingSession(folder: localVaultFolder(), title: title, started: startedAt, deviceID: phoneDeviceID(), transcriptOnly: transcriptOnly, retention: retention, kind: kind)
            // Kept out of housekeeping until it ends, then moved into the chosen folder
            InProgress.add(s.id)
            PendingFolders().set(s.id, folderID)
            session = s

            let input = engine.inputNode
            let inputFormat = input.outputFormat(forBus: 0)
            guard let tapConverter = TapConverter(from: inputFormat, to: format16k) else {
                throw URLError(.cannotDecodeRawData)
            }
            if let audioURL = s.audioURL {
                // AAC that stays playable up to the last second if the app is ended (PI9, PI23)
                file = try RecordingAudio.makeFile(at: audioURL)
            }
            tapInstalled = true
            input.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { [weak self] buffer, _ in
                guard let converted = tapConverter.convert(buffer) else { return }
                Task { @MainActor in self?.handle(converted) }
            }
            observe()
            try engine.start()
            try session?.save()
            meetingID = s.id
            phase = .recording
            if cancelled {
                // Closed in the instant the microphone came on: save the empty start and stop
                await stop()
                return
            }
            await applyHeat(ProcessInfo.processInfo.thermalState)
            clockTask = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(250))
                    self?.tick()
                }
            }
        } catch {
            // Nothing was recorded: leave no empty meeting and no microphone in use
            if tapInstalled { engine.inputNode.removeTap(onBus: 0) }
            engine.stop()
            removeObservers()
            removeThermalObserver()
            file = nil
            if let s = session {
                try? FileManager.default.removeItem(at: s.noteURL)
                if let audio = s.audioURL { try? FileManager.default.removeItem(at: audio) }
                PendingFolders().set(s.id, nil)
                InProgress.remove(s.id)
            }
            session = nil
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            phase = .failed("The recording could not start.")
        }
    }

    /// A call or another app taking the microphone, or the microphone itself changing (a headset
    /// connected or removed, which stops the audio engine): stop and keep what was recorded.
    private func observe() {
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            let began = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt) == AVAudioSession.InterruptionType.began.rawValue
            if began { Task { @MainActor in await self?.stop(note: "The recording stopped because another app or a call took the microphone. What was recorded is saved.") } }
        })
        // Heat is watched until the transcript is finished, not only while recording
        thermalObserver = center.addObserver(forName: ProcessInfo.thermalStateDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in await self?.applyHeat(ProcessInfo.processInfo.thermalState) }
        }
        observers.append(center.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            Task { @MainActor in await self?.stop(note: "The recording stopped because the microphone changed (a headset was connected or removed). What was recorded is saved.") }
        })
    }

    private func removeObservers() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
    }

    private func removeThermalObserver() {
        if let thermalObserver { NotificationCenter.default.removeObserver(thermalObserver) }
        thermalObserver = nil
    }

    /// Reacts to the phone's heat (PI22). While recording: hold transcription at "serious", stop
    /// and save at "critical". While finishing: "serious" still finishes the last lines, "critical"
    /// waits until it cools. Takes the state as a value so tests can pass one.
    func applyHeat(_ state: ProcessInfo.ThermalState) async {
        let action = heatAction(state)
        guard action != heat else { return }
        heat = action
        switch action {
        case .normal:
            heatNote = nil
            drain()
        case .hold:
            heatNote = phase == .finishing ? nil : heatHoldNote
            drain()
        case .stop:
            if phase == .recording || phase == .paused {
                await stop(note: heatStopNote)
            }
            heatNote = phase == .finishing ? heatFinishNote : nil
        }
    }

    private func tick() {
        guard phase == .recording else { return }
        elapsed = recordedSeconds
    }

    /// Keeps a converted buffer in the file (unless transcript only) and feeds the pieces. Buffers
    /// already on their way when pause is pressed still count.
    private func handle(_ converted: Converted) {
        guard phase == .recording || phase == .paused else { return }
        try? file?.write(from: converted.buffer)
        let samples = converted.samples
        var peak: Float = 0
        for s in samples { peak = max(peak, abs(s)) }
        level = level * 0.6 + peak * 0.4
        queue += segmenter.push(samples).map(HeldPiece.init)
        afterArrival()
    }

    /// New pieces: transcribe them, unless the phone is warm; a hold that grows past half an hour
    /// stops the recording rather than keep growing in memory
    private func afterArrival() {
        if heat == .hold, phase == .recording || phase == .paused, backlogSeconds >= heatHoldLimitSeconds {
            Task { await stop(note: heatLongNote) }
            return
        }
        drain()
    }

    /// Whether pieces may be transcribed now: always when cool; while finishing also at "serious"
    private var mayTranscribe: Bool { heat == .normal || (heat == .hold && phase == .finishing) }

    private func drain() {
        guard !working, !queue.isEmpty, mayTranscribe else { return }
        working = true
        let piece = queue.removeFirst()
        workingSeconds = piece.end - piece.start
        let samples = piece.samples
        let transcribe = transcribe
        Task {
            // One retry: the Neural Engine can refuse a request while the phone is busy
            var text = try? await transcribe(samples)
            if text == nil { text = try? await transcribe(samples) }
            if let text {
                do {
                    try session?.add(text: text, start: piece.start, end: piece.end)
                    if let segs = session?.segments { lines = segs }
                } catch {
                    interruptionNote = "Quire could not save the latest line. Check that the iPhone has free space."
                }
            } else {
                interruptionNote = "A few seconds at \(clock(piece.start)) could not be transcribed."
            }
            working = false
            workingSeconds = 0
            if queue.isEmpty && phase == .finishing {
                finishSave()
            } else {
                drain()
            }
        }
    }

    func pause() {
        guard phase == .recording else { return }
        engine.pause()
        pausedAt = Date()
        elapsed = recordedSeconds
        if let rest = segmenter.flush() { queue.append(HeldPiece(rest)) }
        drain()
        phase = .paused
    }

    func resume() {
        guard phase == .paused, let pausedAt else { return }
        pausedTotal += Date().timeIntervalSince(pausedAt)
        self.pausedAt = nil
        do {
            try engine.start()
            phase = .recording
        } catch {
            phase = .paused
            interruptionNote = "The recording could not continue. What was recorded is saved; stop to finish it."
        }
    }

    func bookmark() {
        try? session?.bookmark(at: recordedSeconds)
    }

    func stop(note: String? = nil) async {
        guard phase == .recording || phase == .paused else { return }
        // Finishing the last lines may take a few seconds; keep running if the app goes to the background
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Finish the transcript") { [weak self] in
            Task { @MainActor in self?.endBackgroundTask() }
        }
        elapsed = recordedSeconds
        if tapInstalled { engine.inputNode.removeTap(onBus: 0) }
        engine.stop()
        removeObservers()
        clockTask?.cancel()
        file = nil
        if let rest = segmenter.flush() { queue.append(HeldPiece(rest)) }
        phase = .finishing
        if let note { interruptionNote = note }
        // At "serious" the last lines are finished now; at "critical" they wait for the phone to cool
        heatNote = heat == .stop ? heatFinishNote : nil
        if !working && queue.isEmpty { finishSave() } else { drain() }
    }

    private func finishSave() {
        do {
            try session?.finish(at: Date(), duration: elapsed)
        } catch {
            interruptionNote = "Quire could not save the end of the recording. Check that the iPhone has free space."
        }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        if let id = session?.id { InProgress.remove(id) }
        removeThermalObserver()
        heatNote = nil
        phase = .done
        endBackgroundTask()
    }

    private func endBackgroundTask() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }

    /// Transcribes a recording from Files or Voice Memos into a new meeting (F-IOS-REC-4). Runs
    /// away from the main thread, so a long file does not freeze the screen.
    nonisolated static func importRecording(from url: URL, transcriptOnly: Bool, transcriber: Transcriber, modelDirectory: URL, retention: Retention = .keep, folderID: String? = nil) async throws -> String {
        let accessing = url.startAccessingSecurityScopedResource()
        defer { if accessing { url.stopAccessingSecurityScopedResource() } }
        try await transcriber.load(from: modelDirectory)
        let samples = try readSamples(from: url)
        let created = (try? url.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? Date()
        var session = RecordingSession(folder: localVaultFolder(), title: url.deletingPathExtension().lastPathComponent, started: created, deviceID: phoneDeviceID(), transcriptOnly: transcriptOnly, imported: true, retention: retention)
        InProgress.add(session.id)
        defer { InProgress.remove(session.id) }
        PendingFolders().set(session.id, folderID)
        if let audioURL = session.audioURL {
            // Kept as AAC beside the meeting, like a recording (PI9)
            try? await exportAAC(samples: samples, to: audioURL)
        }
        var segmenter = PauseSegmenter()
        var pieces = segmenter.push(samples)
        if let rest = segmenter.flush() { pieces.append(rest) }
        for piece in pieces {
            let text = try await transcriber.transcribe(piece.samples)
            try session.add(text: text, start: piece.start, end: piece.end)
        }
        try session.finish(at: Date(), duration: Double(samples.count) / 16_000)
        return session.id
    }

    nonisolated static func exportAAC(samples: [Float], to url: URL) async throws {
        let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
        let out = try RecordingAudio.makeFile(at: url)
        var offset = 0
        while offset < samples.count {
            let n = min(16_384, samples.count - offset)
            let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(n))!
            buffer.frameLength = AVAudioFrameCount(n)
            samples.withUnsafeBufferPointer { src in
                buffer.floatChannelData![0].update(from: src.baseAddress! + offset, count: n)
            }
            try out.write(from: buffer)
            offset += n
        }
    }
}
