// Small shared pieces of the screens: one-tap copy (the owner's request, 2026-11-03), the section
// tabs, and the decorative landscapes from the concept (hidden from VoiceOver).

import SwiftUI
import UIKit
import VaultKit

/// A transcript as plain text: one `[mm:ss] Speaker: text` line per segment
func transcriptText(_ segments: [Segment], speakerName: (Int64?) -> String?) -> String {
    segments.map { segment in
        let name = speakerName(segment.speaker).map { "\($0): " } ?? ""
        return "[\(clock(segment.start))] \(name)\(segment.text)"
    }.joined(separator: "\n")
}

/// Copies `text` in one tap and says so for two seconds
struct CopyButton: View {
    let text: String
    /// What is copied, for VoiceOver: "transcript", "summary"
    let what: String
    @State private var copied = false

    var body: some View {
        Button {
            UIPasteboard.general.string = text
            copied = true
            Task {
                try? await Task.sleep(for: .seconds(2))
                copied = false
            }
        } label: {
            Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 12).padding(.vertical, 6)
                .background(Theme.surface, in: Capsule())
                .overlay(Capsule().stroke(Theme.line))
        }
        .tint(Theme.ink)
        .disabled(text.isEmpty)
        .accessibilityLabel(copied ? "\(what.prefix(1).uppercased() + what.dropFirst()) copied" : "Copy the \(what)")
    }
}

/// Transcript, Summary, Notes: a pill control in the app's colours, as in the concept
struct SectionTabs: View {
    @Binding var selection: Int
    let titles: [String]

    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(titles.enumerated()), id: \.offset) { index, title in
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { selection = index }
                } label: {
                    Text(title)
                        .font(.subheadline.weight(selection == index ? .semibold : .regular))
                        .foregroundStyle(Theme.ink)
                        .frame(maxWidth: .infinity, minHeight: 36)
                        .background(selection == index ? Theme.accent.opacity(0.22) : .clear, in: Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selection == index ? .isSelected : [])
            }
        }
        .padding(4)
        .background(Theme.surface, in: Capsule())
    }
}

/// The sage hill and sun from the concept's home screen
struct HomeLandscape: View {
    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width
            ZStack(alignment: .topLeading) {
                Path { p in
                    p.move(to: .zero)
                    p.addLine(to: CGPoint(x: w * 0.62, y: 0))
                    p.addCurve(to: CGPoint(x: 0, y: 190), control1: CGPoint(x: w * 0.45, y: 70), control2: CGPoint(x: w * 0.2, y: 60))
                    p.closeSubpath()
                }
                .fill(Theme.sage.opacity(0.45))
                Path { p in
                    p.move(to: CGPoint(x: w * 0.5, y: 0))
                    p.addCurve(to: CGPoint(x: 0, y: 150), control1: CGPoint(x: w * 0.36, y: 52), control2: CGPoint(x: w * 0.14, y: 44))
                }
                .stroke(Theme.paper.opacity(0.5), lineWidth: 1.2)
                Circle().fill(Theme.accent.opacity(0.85)).frame(width: 30, height: 30)
                    .offset(x: w / 2 - 15, y: 92)
                    .mask(Rectangle().frame(height: 15).offset(y: -7.5))
            }
        }
        .frame(height: 200)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// The terracotta hills along the bottom of the recording screen
struct RecordingHills: View {
    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width, h = geo.size.height
            ZStack {
                Path { p in
                    p.move(to: CGPoint(x: 0, y: h * 0.55))
                    p.addCurve(to: CGPoint(x: w, y: h * 0.25), control1: CGPoint(x: w * 0.35, y: h * 0.2), control2: CGPoint(x: w * 0.6, y: h * 0.65))
                    p.addLine(to: CGPoint(x: w, y: h)); p.addLine(to: CGPoint(x: 0, y: h)); p.closeSubpath()
                }
                .fill(Theme.accent.opacity(0.18))
                Path { p in
                    p.move(to: CGPoint(x: 0, y: h * 0.8))
                    p.addCurve(to: CGPoint(x: w, y: h * 0.5), control1: CGPoint(x: w * 0.4, y: h * 0.45), control2: CGPoint(x: w * 0.7, y: h * 0.9))
                    p.addLine(to: CGPoint(x: w, y: h)); p.addLine(to: CGPoint(x: 0, y: h)); p.closeSubpath()
                }
                .fill(Theme.accent.opacity(0.3))
                Circle().fill(Theme.accent.opacity(0.8)).frame(width: 22, height: 22).position(x: w * 0.82, y: h * 0.3)
            }
        }
        .frame(height: 150)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Soft rings that pulse out from the record button and fade (the owner's request, 2026-11-03).
/// Still when Reduce Motion is on.
struct PulseRings: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var out = false

    var body: some View {
        ZStack {
            ForEach(0..<2, id: \.self) { i in
                Circle()
                    .fill(Theme.accent.opacity(0.14))
                    .frame(width: 136, height: 136)
                    .scaleEffect(out ? 1.45 : 1)
                    .opacity(out ? 0 : 1)
                    .animation(reduceMotion ? nil : .easeOut(duration: 2.4).repeatForever(autoreverses: false).delay(Double(i) * 1.2), value: out)
            }
        }
        .onAppear { if !reduceMotion { out = true } }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// The longer explanation of a Settings section, folded away until asked for (plan PN5).
struct MoreAbout: View {
    let text: String
    @State private var open = false

    init(_ text: String) { self.text = text }

    // One labelled button (a DisclosureGroup adds an unlabelled arrow of its own)
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation { open.toggle() }
            } label: {
                Label("More about this", systemImage: open ? "chevron.down" : "chevron.right")
                    .font(.footnote)
                    .foregroundStyle(Theme.accentStrong)
            }
            .accessibilityValue(open ? "Open" : "Closed")
            if open {
                Text(text).font(.footnote).foregroundStyle(Theme.inkMuted)
            }
        }
    }
}
