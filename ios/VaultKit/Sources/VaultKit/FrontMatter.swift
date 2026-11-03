// The strict front matter grammar (sync-format.md section 4), byte for byte the same as the
// desktop's reader and writer. Works on bytes, not Swift Characters, because a Character can
// join "\r\n" into one.

import Foundation

public enum Value: Equatable, Sendable {
    case str(String)
    case int(Int64)
    case bool(Bool)
    case null
    case list([Value])
    case map([(String, Value)])

    public static func == (a: Value, b: Value) -> Bool {
        switch (a, b) {
        case let (.str(x), .str(y)): return x == y
        case let (.int(x), .int(y)): return x == y
        case let (.bool(x), .bool(y)): return x == y
        case (.null, .null): return true
        case let (.list(x), .list(y)): return x == y
        case let (.map(x), .map(y)): return x.count == y.count && zip(x, y).allSatisfy { $0.0 == $1.0 && $0.1 == $1.1 }
        default: return false
        }
    }

    /// A string value in NFC, as writers must emit it.
    public static func string(_ text: String) -> Value { .str(nfc(text)) }
    public static func optionalString(_ text: String?) -> Value { text.map(string) ?? .null }

    public var stringValue: String? { if case let .str(s) = self { return s } else { return nil } }
    public var intValue: Int64? { if case let .int(n) = self { return n } else { return nil } }
    public subscript(key: String) -> Value? {
        if case let .map(entries) = self { return entries.first { $0.0 == key }?.1 }
        return nil
    }
}

public struct FormatError: Error, Equatable, CustomStringConvertible {
    public let description: String
    init(_ description: String) { self.description = description }
}

public func nfc(_ text: String) -> String { text.precomposedStringWithCanonicalMapping }

func isControl(_ scalar: Unicode.Scalar) -> Bool { scalar.properties.generalCategory == .control }

/// A double-quoted string with only the escapes the grammar allows; never `\/`.
public func quote(_ value: String) -> String {
    var out = "\""
    for scalar in value.unicodeScalars {
        switch scalar {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        default:
            if isControl(scalar) {
                out += String(format: "\\u%04x", scalar.value)
            } else {
                out.unicodeScalars.append(scalar)
            }
        }
    }
    return out + "\""
}

private func scalarText(_ value: Value) -> String? {
    switch value {
    case let .str(s): return quote(s)
    case let .int(n): return String(n)
    case let .bool(b): return b ? "true" : "false"
    case .null: return "null"
    case let .list(items) where items.isEmpty: return "[]"
    case let .map(entries) where entries.isEmpty: return "{}"
    default: return nil
    }
}

private func writeEntries(_ out: inout String, _ entries: [(String, Value)], _ indent: Int, firstPrefix: String? = nil) {
    for (i, (key, value)) in entries.enumerated() {
        let lead = (i == 0 ? firstPrefix : nil) ?? String(repeating: " ", count: indent)
        if let text = scalarText(value) {
            out += "\(lead)\(key): \(text)\n"
        } else {
            out += "\(lead)\(key):\n"
            writeBlock(&out, value, indent + 2)
        }
    }
}

private func writeBlock(_ out: inout String, _ value: Value, _ indent: Int) {
    switch value {
    case let .map(entries): writeEntries(&out, entries, indent)
    case let .list(items):
        for item in items {
            let dash = String(repeating: " ", count: indent) + "- "
            if case let .map(entries) = item, !entries.isEmpty {
                writeEntries(&out, entries, indent + 2, firstPrefix: dash)
            } else {
                out += dash + (scalarText(item) ?? "null") + "\n"
            }
        }
    default: break
    }
}

/// `---`, the entries in the given order, `---`, then the body exactly as given.
public func writeDocument(_ entries: [(String, Value)], body: String) -> String {
    var out = "---\n"
    writeEntries(&out, entries, 0)
    return out + "---\n" + body
}

private struct Line {
    let indent: Int
    let text: String
}

/// Decodes UTF-8 exactly, keeping a leading U+FEFF (Foundation's decoder would drop it).
func decodeUTF8<C: Collection<UInt8>>(_ bytes: C) -> String? { String(validating: bytes, as: UTF8.self) }

private func hasScalarPrefix(_ text: String, _ prefix: String) -> Bool { text.unicodeScalars.starts(with: prefix.unicodeScalars) }
private func hasScalarSuffix(_ text: String, _ suffix: String) -> Bool { text.unicodeScalars.reversed().starts(with: suffix.unicodeScalars.reversed()) }
private func dropScalars(_ text: String, _ n: Int) -> String { String(String.UnicodeScalarView(text.unicodeScalars.dropFirst(n))) }

/// The deepest nesting a reader follows; real files use 3 levels.
private let maxDepth = 8

func keyOK(_ key: Substring) -> Bool {
    guard let first = key.unicodeScalars.first, ("a"..."z").contains(first) else { return false }
    return key.unicodeScalars.allSatisfy { ("a"..."z").contains($0) || ("0"..."9").contains($0) || $0 == "_" }
}

private func parseString(_ text: String) throws -> String {
    let scalars = Array(text.unicodeScalars.dropFirst().dropLast())
    var out = String.UnicodeScalarView()
    var i = 0
    while i < scalars.count {
        let c = scalars[i]
        i += 1
        if c == "\"" { throw FormatError("unescaped quote in a string") }
        if isControl(c) { throw FormatError("a control character must be escaped") }
        guard c == "\\" else { out.append(c); continue }
        guard i < scalars.count else { throw FormatError("escape not allowed") }
        let e = scalars[i]
        i += 1
        switch e {
        case "\"": out.append("\"")
        case "\\": out.append("\\")
        case "n": out.append("\n")
        case "r": out.append("\r")
        case "t": out.append("\t")
        case "u":
            guard i + 4 <= scalars.count else { throw FormatError("bad \\u escape") }
            let hex = String(String.UnicodeScalarView(scalars[i..<i + 4]))
            guard hex.unicodeScalars.allSatisfy({ $0.properties.isASCIIHexDigit }), let code = UInt32(hex, radix: 16), let ch = Unicode.Scalar(code) else {
                throw FormatError("bad \\u escape")
            }
            out.append(ch)
            i += 4
        default: throw FormatError("escape not allowed")
        }
    }
    return String(out)
}

private func parseScalar(_ text: String) throws -> Value {
    switch text {
    case "true": return .bool(true)
    case "false": return .bool(false)
    case "null": return .null
    case "[]": return .list([])
    case "{}": return .map([])
    default: break
    }
    if text.unicodeScalars.count >= 2, hasScalarPrefix(text, "\""), hasScalarSuffix(text, "\"") {
        return .str(try parseString(text))
    }
    let digits = hasScalarPrefix(text, "-") ? dropScalars(text, 1) : text
    if !digits.isEmpty, digits.unicodeScalars.allSatisfy({ ("0"..."9").contains($0) }) {
        guard let n = Int64(text) else { throw FormatError("integer out of range") }
        return .int(n)
    }
    // Never quote the value: it can be part of a title
    throw FormatError("value not allowed")
}

private func splitEntry(_ text: String) throws -> (String, String?) {
    let scalars = Array(text.unicodeScalars)
    guard let colon = scalars.firstIndex(of: ":") else { throw FormatError("expected key: value") }
    let key = String(String.UnicodeScalarView(scalars[..<colon]))
    guard keyOK(Substring(key)) else { throw FormatError("bad key") }
    let rest = Array(scalars[(colon + 1)...])
    if rest.isEmpty { return (key, nil) }
    guard rest.count > 1, rest[0] == " ", rest[1] != " " else { throw FormatError("expected one space after the colon") }
    return (key, String(String.UnicodeScalarView(rest[1...])))
}

private func isItem(_ text: String) -> Bool { hasScalarPrefix(text, "- ") }

private struct Parser {
    let lines: [Line]
    var i = 0

    mutating func child(_ indent: Int) throws -> Value {
        guard indent / 2 <= maxDepth else { throw FormatError("nested too deeply") }
        guard i < lines.count, lines[i].indent == indent else { throw FormatError("an empty value must be written as [] or {}") }
        return isItem(lines[i].text) ? try list(indent) : try map(indent, first: nil)
    }

    mutating func map(_ indent: Int, first: String?) throws -> Value {
        var entries: [(String, Value)] = []
        var pending = first
        while true {
            let text: String
            if let p = pending {
                text = p
                pending = nil
            } else if i < lines.count, lines[i].indent == indent, !isItem(lines[i].text) {
                text = lines[i].text
                i += 1
            } else if i < lines.count, lines[i].indent > indent {
                throw FormatError("unexpected indentation")
            } else {
                break
            }
            let (key, value) = try splitEntry(text)
            if entries.contains(where: { $0.0 == key }) { throw FormatError("duplicate key") }
            entries.append((key, try value.map(parseScalar) ?? child(indent + 2)))
        }
        return .map(entries)
    }

    mutating func list(_ indent: Int) throws -> Value {
        var items: [Value] = []
        while i < lines.count {
            let line = lines[i]
            if line.indent != indent || !isItem(line.text) {
                if line.indent > indent { throw FormatError("unexpected indentation") }
                break
            }
            i += 1
            let rest = dropScalars(line.text, 2)
            let scalars = Array(rest.unicodeScalars)
            let looksLikeEntry = scalars.firstIndex(of: ":").map { keyOK(Substring(String(String.UnicodeScalarView(scalars[..<$0])))) } ?? false
            guard indent / 2 < maxDepth else { throw FormatError("nested too deeply") }
            items.append(looksLikeEntry && !hasScalarPrefix(rest, "\"") ? try map(indent + 2, first: rest) : try parseScalar(rest))
        }
        return .list(items)
    }
}

/// Splits a file into its front matter entries and its body, enforcing the grammar exactly.
/// Accepts a byte order mark and CRLF line endings in the front matter; the body is kept as is.
public func parseDocument(_ data: Data) throws -> ([(String, Value)], String) {
    var bytes = [UInt8](data)
    if bytes.starts(with: [0xEF, 0xBB, 0xBF]) { bytes.removeFirst(3) }
    var offset: Int
    if bytes.starts(with: Array("---\n".utf8)) {
        offset = 4
    } else if bytes.starts(with: Array("---\r\n".utf8)) {
        offset = 5
    } else {
        throw FormatError("missing front matter")
    }
    var lines: [Line] = []
    var end: Int?
    while offset < bytes.count {
        let newline = bytes[offset...].firstIndex(of: 0x0A)
        let lineEnd = newline ?? bytes.count
        var raw = Array(bytes[offset..<lineEnd])
        offset = newline.map { $0 + 1 } ?? bytes.count
        if raw.last == 0x0D { raw.removeLast() }
        guard let line = decodeUTF8(raw) else { throw FormatError("not UTF-8") }
        if line == "---" {
            end = offset
            break
        }
        if raw.contains(0x09) { throw FormatError("tabs are not allowed") }
        if line.trimmingCharacters(in: .whitespaces).isEmpty { throw FormatError("blank lines are not allowed in front matter") }
        let indent = raw.prefix { $0 == 0x20 }.count
        if indent % 2 != 0 { throw FormatError("indentation must be 2 spaces per level") }
        guard let text = decodeUTF8(raw.dropFirst(indent)) else { throw FormatError("not UTF-8") }
        lines.append(Line(indent: indent, text: text))
    }
    guard let end else { throw FormatError("front matter is not closed") }
    var parser = Parser(lines: lines)
    guard case let .map(entries) = try parser.map(0, first: nil) else { throw FormatError("front matter") }
    if parser.i != lines.count { throw FormatError("unexpected line in front matter") }
    guard let body = decodeUTF8(bytes[end...]) else { throw FormatError("body is not UTF-8") }
    return (entries, body)
}
