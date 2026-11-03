/** Helpers for Ask answers (spec F-QA-4, F-QA-6). */

export interface AskCitation {
  label: string;
  seconds: number;
}

const CITATION = /\[(\d{1,2}:\d{2}(?::\d{2})?)\](?!\()/g;

/** Turn each kept citation into a `#t=` link the answer view renders as a jump. */
export function linkCitations(answer: string, citations: AskCitation[]): string {
  const seconds = new Map(citations.map((c) => [c.label, c.seconds]));
  return answer.replace(CITATION, (whole, label: string) => {
    const at = seconds.get(label);
    return at === undefined ? whole : `[${label}](#t=${at})`;
  });
}

/** Seconds from a `#t=` link, or null when the link is anything else. */
export function parseJumpHref(href: string | undefined): number | null {
  const match = /^#t=(\d+(?:\.\d+)?)$/.exec(href ?? '');
  return match ? Number(match[1]) : null;
}

/** The id of the line that holds `seconds`, or the last line that starts before it. */
export function segmentAtTime(
  segments: { id: string; timestamp: number; endTime?: number }[],
  seconds: number,
): string | undefined {
  let best: { id: string; start: number } | undefined;
  for (const s of segments) {
    const start = Math.floor(s.timestamp);
    if (start <= seconds && (!best || start >= best.start)) best = { id: s.id, start };
  }
  return best?.id;
}

/** True when `seconds` belongs to a citation the app kept (spec F-QA-4). */
export function isKeptCitation(seconds: number, citations: AskCitation[]): boolean {
  return citations.some((c) => c.seconds === seconds);
}

/**
 * What "Add to note" appends: the question and answer, labelled as AI generated. Images, links
 * and raw HTML from the model are dropped (link text stays), so the note never points anywhere.
 */
export function noteBlockMarkdown(question: string, answer: string): string {
  const plain = answer
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\(#t=[^)]*\)/g, '[$1]')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, '');
  return `**Ask (AI generated):** ${question.trim()}\n\n${plain.trim()}`;
}
