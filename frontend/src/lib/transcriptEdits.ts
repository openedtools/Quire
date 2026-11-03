// Line editing and the transcript check (plan PL5). The backend keeps each changed line's first
// text, so every change can be undone.

import { invoke } from '@tauri-apps/api/core';

export interface LineEdit {
  lineId: string;
  original: string;
}

export interface Suggestion {
  lineId: string;
  start: number;
  lineText: string;
  find: string;
  replace: string;
  reason: string;
}

export const editLine = (meetingId: string, lineId: string, text: string) =>
  invoke<void>('api_edit_transcript_line', { meetingId, lineId, text });
export const undoLine = (meetingId: string, lineId: string) => invoke<string>('api_undo_transcript_line', { meetingId, lineId });
/** Applies one suggestion to the line as it is now, so earlier fixes to it stay; returns the new text. */
export const applySuggestion = (meetingId: string, s: Pick<Suggestion, 'lineId' | 'find' | 'replace'>) =>
  invoke<string>('api_apply_suggestion', { meetingId, lineId: s.lineId, find: s.find, replace: s.replace });
export const replaceEverywhere = (meetingId: string, find: string, replace: string) =>
  invoke<number>('api_replace_in_transcript', { meetingId, find, replace });
export const lineEdits = (meetingId: string) => invoke<LineEdit[]>('api_transcript_edits', { meetingId });
export const checkTranscript = (meetingId: string) => invoke<Suggestion[]>('api_check_transcript', { meetingId });

/** Sent when lines change outside the line itself (the check), so the Edited marks follow */
export const EDITED_EVENT = 'quire:transcript-edited';
export const announceEdits = () => window.dispatchEvent(new Event(EDITED_EVENT));

/** The line with the suggestion applied (its first occurrence). */
export function applyFix(text: string, find: string, replace: string): string {
  const at = text.indexOf(find);
  return at < 0 ? text : text.slice(0, at) + replace + text.slice(at + find.length);
}

/** Suggestions that repeat the same change, keyed `find -> replace`, with how often each occurs. */
export function repeatedKinds(suggestions: Suggestion[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const s of suggestions) counts.set(kindOf(s), (counts.get(kindOf(s)) ?? 0) + 1);
  return new Map([...counts].filter(([, n]) => n > 1));
}

export const kindOf = (s: Pick<Suggestion, 'find' | 'replace'>) => `${s.find}\u0000${s.replace}`;

/** [mm:ss] for a time in seconds. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `[${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}]`;
}
