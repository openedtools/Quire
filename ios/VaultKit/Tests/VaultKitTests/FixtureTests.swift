// AC-IOS-1: the shared fixtures in docs/vault-fixtures, the same ones the desktop tests.

import Foundation
import Testing
@testable import VaultKit

private let fixtures = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .appendingPathComponent("docs/vault-fixtures")

private func files(under root: URL) -> [String] {
    let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey])
    var out: [String] = []
    while let url = enumerator?.nextObject() as? URL {
        if (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true {
            out.append(String(url.path.dropFirst(root.path.count + 1)))
        }
    }
    return out.sorted()
}

@Test func everyCanonicalFileReadsBackByteForByte() throws {
    let root = fixtures.appendingPathComponent("canonical")
    let rels = files(under: root)
    #expect(rels.count >= 7)
    for rel in rels {
        let data = try Data(contentsOf: root.appendingPathComponent(rel))
        let rewritten: String
        if rel.hasSuffix(".transcript.json") {
            let (id, segments) = try parseSidecar(data)
            rewritten = renderSidecar(noteID: id, segments: segments)
        } else if rel.hasSuffix("_folder.json") {
            rewritten = renderFolder(try parseFolder(data))
        } else if rel == "quire-format.json" {
            rewritten = renderMarker(try parseMarker(data))
        } else if rel.hasPrefix("_people/") {
            rewritten = renderPerson(try parsePerson(data))
        } else {
            let note = try parseNote(data)
            if let t = note.transcript {
                let dir = (rel as NSString).deletingLastPathComponent
                let side = try Data(contentsOf: root.appendingPathComponent(dir).appendingPathComponent(t.file))
                #expect(sha256Hex(side) == t.sha256, "the note's commit point matches its sidecar")
                #expect(t.file == sidecarName(noteID: note.id, sha256: t.sha256))
            }
            rewritten = renderNote(note)
        }
        #expect(Data(rewritten.utf8) == data, "\(rel) does not round trip")
    }
}

@Test func everyGrammarFixtureIsRejected() throws {
    let dir = fixtures.appendingPathComponent("grammar/invalid")
    let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
    #expect(names.count >= 10)
    for name in names {
        let data = try Data(contentsOf: dir.appendingPathComponent(name))
        #expect(throws: FormatError.self, "\(name) must be rejected") { try parseNote(data) }
    }
}

@Test func folderNamesMatchTheSharedTable() throws {
    let data = try Data(contentsOf: fixtures.appendingPathComponent("names.json"))
    let cases = try #require(try JSONSerialization.jsonObject(with: data) as? [[String]])
    #expect(cases.count >= 10)
    for pair in cases {
        #expect(folderDirName(pair[0]) == pair[1], "\(pair[0])")
    }
}

@Test func collisionsGiveTheLargerIdTheSuffix() {
    let dirs = folderDirs([(id: "01JC5K3N7P9R2T4V6X8Z0BDFGK", name: "Work?"), (id: "01JC5K3N7P9R2T4V6X8Z0BDFGH", name: "Work")])
    #expect(dirs[0].dir == "Work")
    #expect(dirs[1].dir == "Work (2)")
}

@Test func theGrammarMatchesTheDesktopOnEdgeCases() throws {
    func ok(_ text: String) -> Bool { (try? parseDocument(Data(text.utf8))) != nil }
    #expect(ok("---\nquire: 1\n---\n"))
    #expect(ok("\u{feff}---\r\nquire: 1\r\n---\r\nbody\r\n"), "a BOM and CRLF are accepted")
    let (_, body) = try parseDocument(Data("---\r\nquire: 1\r\n---\r\nbody\r\n".utf8))
    #expect(body == "body\r\n", "the body is kept exactly")
    #expect(!ok("---\na: \"\\u+041\"\n---\n"))
    #expect(!ok("---\na: \"x\u{07}y\"\n---\n"), "a raw control character")
    #expect(ok("---\na: \"\\u00e9: - x\"\nb: -12\nc_2: \"\"\n---\n"))
    #expect(isULID("01JC90A2T8N4Y6B3D7F1GHKQ5S"))
    #expect(!isULID("01JC90A2T8N4Y6B3D7F1GHKQ5I"))
    #expect(!isULID("81JC90A2T8N4Y6B3D7F1GHKQ5S"))
}
