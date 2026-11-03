// The "Read more" page from the consent reminder (plan PI24, spec F-IOS-PRIV-3): the desktop's
// guidance page word for word ("computer" reads "iPhone"), with the two sample lines to copy.
// General guidance, never a claim that anything is allowed.

import SwiftUI
import UIKit

let consentSpokenLine = "I'm using a tool to transcribe this meeting so I can take notes. Is that okay with everyone?"
let consentInvitationLine = "I'll be using a local transcription tool during this meeting. Reply if you'd rather I didn't."
private let guideURL = URL(string: "https://www.rcfp.org/reporters-recording-guide/")!

struct ConsentGuideView: View {
    @State private var copied: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Text("Telling people you're recording").font(.serifTitle(28)).foregroundStyle(Theme.ink)
                    .accessibilityAddTraits(.isHeader)
                section("Why it matters", bullets: [
                    "Laws about recording conversations differ between states and countries.",
                    "Some laws cover learning what is said in a conversation, not only recording it.",
                    "People on a call may be somewhere with different rules than where you are.",
                ])
                section("What to do", bullets: [
                    "Say it out loud at the start.",
                    "Ask for a yes.",
                    "Put it in the invitation.",
                    "Stop if someone objects.",
                    "Follow your workplace rules.",
                ])
                VStack(alignment: .leading, spacing: 10) {
                    heading("Words you can use")
                    copyLine("Spoken line", consentSpokenLine)
                    copyLine("Invitation line", consentInvitationLine)
                }
                VStack(alignment: .leading, spacing: 8) {
                    heading("Transcript only")
                    Text("Not saving audio keeps less on your iPhone. It does not change whether you need people's consent.").foregroundStyle(Theme.ink)
                    Text("Transcript only saves no audio file. Whether you need people's consent depends on where you and they are, and it does not change with this setting.")
                        .font(.subheadline).foregroundStyle(Theme.inkMuted)
                }
                VStack(alignment: .leading, spacing: 8) {
                    heading("State by state")
                    Text("The Reporters Committee for Freedom of the Press keeps a guide to recording laws in each US state.").foregroundStyle(Theme.ink)
                    Link(destination: guideURL) {
                        Label("Open the Reporter's Recording Guide", systemImage: "arrow.up.right.square")
                    }
                    .tint(Theme.accentStrong)
                    Text("Opens in Safari.").font(.caption).foregroundStyle(Theme.inkMuted)
                }
                Text("This page is general guidance, not legal advice.").font(.subheadline).foregroundStyle(Theme.inkMuted)
            }
            .padding(20)
        }
        .background(Theme.paper)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func heading(_ text: String) -> some View {
        Text(text).font(.headline).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)
    }

    private func section(_ title: String, bullets: [String]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            heading(title)
            ForEach(bullets, id: \.self) { item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text("•").accessibilityHidden(true)
                    Text(item)
                }
                .foregroundStyle(Theme.ink)
            }
        }
    }

    private func copyLine(_ label: String, _ text: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.caption.weight(.medium)).foregroundStyle(Theme.inkMuted)
            HStack(alignment: .top, spacing: 12) {
                Text(text).foregroundStyle(Theme.ink)
                Spacer()
                Button {
                    UIPasteboard.general.string = text
                    copied = label
                } label: {
                    Image(systemName: copied == label ? "checkmark" : "doc.on.doc").frame(width: 44, height: 44)
                }
                .accessibilityLabel(copied == label ? "\(label) copied" : "Copy the \(label.lowercased())")
                .tint(Theme.ink)
            }
        }
        .padding(12)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Theme.line))
    }
}
