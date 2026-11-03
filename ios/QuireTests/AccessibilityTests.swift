// AC-IOS-10, F-IOS-UI-1: every text and background pair the screens use passes WCAG 2.2 AA in
// light and dark (4.5:1 for text, 3:1 for icons and large shapes), and the colours are still the
// desktop's tokens.

import Foundation
import SwiftUI
import Testing
import UIKit
import VaultKit
@testable import Quire

private func rgb(_ color: Color, dark: Bool) -> [Double] {
    let resolved = UIColor(color).resolvedColor(with: UITraitCollection(userInterfaceStyle: dark ? .dark : .light))
    var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
    resolved.getRed(&r, green: &g, blue: &b, alpha: &a)
    return [r, g, b].map { ($0 * 255).rounded() }
}

private func luminance(_ c: [Double]) -> Double {
    let f = c.map { v -> Double in
        let s = v / 255
        return s <= 0.03928 ? s / 12.92 : pow((s + 0.055) / 1.055, 2.4)
    }
    return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2]
}

private func contrast(_ a: [Double], _ b: [Double]) -> Double {
    let (hi, lo) = (max(luminance(a), luminance(b)), min(luminance(a), luminance(b)))
    return (hi + 0.05) / (lo + 0.05)
}

/// The pairs the screens draw: (name, foreground, background, minimum ratio)
private let pairs: [(String, Color, Color, Double)] = [
    ("text on paper", Theme.ink, Theme.paper, 4.5),
    ("muted text on paper", Theme.inkMuted, Theme.paper, 4.5),
    ("text on surface", Theme.ink, Theme.surface, 4.5),
    ("muted text on surface", Theme.inkMuted, Theme.surface, 4.5),
    ("button text on the strong accent", Theme.onAccent, Theme.accentStrong, 4.5),
    ("links and tinted buttons on paper", Theme.accentStrong, Theme.paper, 4.5),
    ("links and tinted buttons on surface", Theme.accentStrong, Theme.surface, 4.5),
    ("warnings on paper", Theme.warning, Theme.paper, 4.5),
    ("this-iPhone labels on paper", Theme.sage, Theme.paper, 4.5),
    ("record button icon on the accent", Theme.onAccent, Theme.accent, 3),
    ("record button against paper", Theme.accent, Theme.paper, 3),
    ("strong lines against paper", Theme.lineStrong, Theme.paper, 3),
    ("a line with no speaker", Theme.inkMuted, Theme.surface, 4.5),
    ("warnings on surface (Ask problems on their card)", Theme.warning, Theme.surface, 4.5),
    ("this-iPhone labels on surface", Theme.sage, Theme.surface, 4.5),
    ("warnings on grouped rows", Theme.warning, Color(UIColor.secondarySystemGroupedBackground), 4.5),
    ("this-iPhone labels on grouped rows", Theme.sage, Color(UIColor.secondarySystemGroupedBackground), 4.5),
    ("muted text on grouped rows", Theme.inkMuted, Color(UIColor.secondarySystemGroupedBackground), 4.5),
] + Theme.speakers.enumerated().map { ("speaker \($0.offset + 1) initial", $0.element.ink, $0.element.fill, 4.5) }

@Test(arguments: [false, true])
func everyPairPassesAA(dark: Bool) {
    for (name, fg, bg, minimum) in pairs {
        let ratio = contrast(rgb(fg, dark: dark), rgb(bg, dark: dark))
        #expect(ratio >= minimum, "\(name) in \(dark ? "dark" : "light"): \(String(format: "%.2f", ratio)):1, needs \(minimum):1")
    }
}

/// The desktop's tokens.css, read from the repository (shared with the phone, never copied by hand)
private func desktopTokens() throws -> (light: [String: [Double]], dark: [String: [Double]]) {
    let css = try String(contentsOf: URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("frontend/src/styles/tokens.css"), encoding: .utf8)
    func read(_ selector: String) -> [String: [Double]] {
        guard let start = css.range(of: "\(selector) {") else { return [:] }
        let block = css[start.upperBound...].prefix { $0 != "}" }
        var out: [String: [Double]] = [:]
        for match in block.matches(of: /--([a-z0-9-]+):\s*(\d{1,3})\s+(\d{1,3})\s+(\d{1,3})\s*;/) {
            out[String(match.1)] = [Double(match.2)!, Double(match.3)!, Double(match.4)!]
        }
        return out
    }
    let light = read(":root")
    return (light, light.merging(read(".dark")) { _, dark in dark })
}

@Test func theColoursAreStillTheDesktopTokens() throws {
    let tokens = try desktopTokens()
    var named: [(String, Color)] = [
        ("paper", Theme.paper), ("surface", Theme.surface), ("line", Theme.line), ("line-strong", Theme.lineStrong),
        ("ink", Theme.ink), ("ink-muted", Theme.inkMuted), ("accent", Theme.accent), ("accent-strong", Theme.accentStrong),
        ("on-accent", Theme.onAccent), ("sage", Theme.sage), ("warning", Theme.warning),
    ]
    named += Theme.speakers.enumerated().flatMap { [("speaker-\($0.offset + 1)", $0.element.fill), ("speaker-\($0.offset + 1)-ink", $0.element.ink)] }
    for (name, color) in named {
        #expect(tokens.light[name] != nil && tokens.dark[name] != nil, "\(name) is in tokens.css")
        if let light = tokens.light[name] { #expect(rgb(color, dark: false) == light, "\(name) light") }
        if let dark = tokens.dark[name] { #expect(rgb(color, dark: true) == dark, "\(name) dark") }
    }
    #expect(tokens.light.count > 5, "tokens.css was read")
}

// ===== One-tap copy (the owner's request, 2026-11-03) =====

@Test func aCopiedTranscriptHasTimesAndSpeakers() {
    let segments = [
        Segment(id: "seg-1", start: 0, end: 4, speaker: 1, text: "Welcome to the pond committee."),
        Segment(id: "seg-2", start: 65, end: 70, speaker: nil, text: "Benches by Friday."),
    ]
    let text = transcriptText(segments) { $0 == 1 ? "Alex Rivera" : nil }
    #expect(text == "[00:00] Alex Rivera: Welcome to the pond committee.\n[01:05] Benches by Friday.")
}
