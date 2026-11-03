// The desktop's design tokens (frontend/src/styles/tokens.css), light and dark, so both apps
// share one look (spec F-IOS-UI-1).

import SwiftUI
import UIKit

private func rgb(_ r: Int, _ g: Int, _ b: Int) -> UIColor {
    UIColor(red: CGFloat(r) / 255, green: CGFloat(g) / 255, blue: CGFloat(b) / 255, alpha: 1)
}

private func token(_ light: UIColor, _ dark: UIColor) -> Color {
    Color(UIColor { $0.userInterfaceStyle == .dark ? dark : light })
}

enum Theme {
    static let paper = token(rgb(250, 246, 238), rgb(31, 27, 23))
    static let surface = token(rgb(242, 235, 221), rgb(42, 37, 32))
    static let surfaceHover = token(rgb(238, 230, 215), rgb(53, 47, 40))
    static let line = token(rgb(228, 218, 202), rgb(58, 51, 43))
    static let lineStrong = token(rgb(137, 125, 109), rgb(133, 122, 107))
    static let ink = token(rgb(43, 38, 34), rgb(239, 232, 220))
    static let inkMuted = token(rgb(107, 97, 85), rgb(168, 157, 142))
    static let accent = token(rgb(196, 98, 63), rgb(224, 138, 102))
    static let accentStrong = token(rgb(169, 78, 43), rgb(224, 138, 102))
    static let onAccent = token(rgb(250, 246, 238), rgb(31, 27, 23))
    static let sage = token(rgb(107, 108, 84), rgb(169, 179, 147))
    static let sageTint = token(rgb(229, 232, 216), rgb(49, 53, 40))
    static let warning = token(rgb(138, 90, 0), rgb(224, 176, 90))
    static let warningTint = token(rgb(246, 232, 204), rgb(58, 46, 23))

    /// The desktop's eight speaker colours: a soft fill behind the initial, with its own ink
    static let speakers: [(fill: Color, ink: Color)] = [
        (token(rgb(244, 225, 215), rgb(65, 46, 37)), token(rgb(150, 79, 44), rgb(204, 150, 123))),
        (token(rgb(225, 229, 234), rgb(46, 50, 56)), token(rgb(77, 98, 128), rgb(148, 163, 184))),
        (token(rgb(232, 234, 225), rgb(53, 56, 46)), token(rgb(93, 103, 66), rgb(164, 173, 138))),
        (token(rgb(237, 222, 226), rgb(59, 43, 47)), token(rgb(138, 66, 84), rgb(194, 147, 159))),
        (token(rgb(240, 232, 219), rgb(61, 54, 41)), token(rgb(126, 94, 37), rgb(198, 165, 108))),
        (token(rgb(238, 223, 221), rgb(60, 44, 42)), token(rgb(148, 68, 56), rgb(201, 149, 141))),
        (token(rgb(237, 235, 222), rgb(58, 57, 44)), token(rgb(107, 102, 46), rgb(186, 180, 120))),
        (token(rgb(242, 236, 217), rgb(64, 57, 38)), token(rgb(125, 98, 18), rgb(214, 184, 92))),
    ]

    /// The solid circle behind a speaker's initial, as in the concept: the desktop's speaker ink,
    /// which passes AA under a light initial; a line with no speaker uses the muted ink
    static func speaker(_ slot: Int64?) -> Color {
        guard let slot, slot > 0 else { return inkMuted }
        return speakers[Int(slot - 1) % speakers.count].ink
    }

    /// The initial on that circle
    static func speakerInk(_ slot: Int64?) -> Color { onAccent }
}

extension Font {
    /// The serif used for page titles, as on the desktop. A text style, so it follows the
    /// person's text size, also when it changes while the app is open
    static func serifTitle(_ size: CGFloat = 30) -> Font {
        .system(size >= 30 ? .largeTitle : .title, design: .serif)
    }
}

/// A rounded card on the paper background.
struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        content
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).stroke(Theme.line))
    }
}
