/**
 * Which summary lines came from the person's own notes (spec F-NOTE-6). Decided here by word
 * overlap, not by asking the model to mark its output.
 */

const STOP_WORDS = new Set([
  'a', 'an', 'and', 'are', 'as', 'at', 'be', 'by', 'for', 'from', 'has', 'have', 'in', 'is', 'it',
  'of', 'on', 'or', 'that', 'the', 'their', 'this', 'to', 'was', 'we', 'were', 'will', 'with',
]);

/** Lowercase content words of a line, without Markdown marks, punctuation or common words. */
export function contentWords(line: string): Set<string> {
  const words = line
    .toLowerCase()
    .replace(/^\s*(?:[-*+]|\d+[.)]|#{1,6}|>)\s*/, '')
    .replace(/[^\p{L}\p{N}\s]/gu, ' ')
    .split(/\s+/)
    .filter((w) => w.length > 1 && !STOP_WORDS.has(w));
  return new Set(words);
}

/** The note lines worth matching: non-empty lines with at least two content words. */
export function noteLinesOf(notesMarkdown: string): Set<string>[] {
  return notesMarkdown
    .split(/\r?\n/)
    .map(contentWords)
    .filter((words) => words.size >= 2);
}

/**
 * True when a summary line carries a note: most of the note's words appear in the line, or most
 * of the line's words appear in the note (a summary often shortens or lengthens the point).
 */
export function isFromNotes(summaryLine: string, noteLines: Set<string>[], threshold = 0.6): boolean {
  const line = contentWords(summaryLine);
  if (line.size < 2) return false;
  return noteLines.some((note) => {
    let shared = 0;
    for (const w of note) if (line.has(w)) shared += 1;
    return shared / note.size >= threshold || shared / line.size >= threshold;
  });
}
