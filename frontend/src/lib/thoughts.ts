// Thought mode on the desktop (plan PN4): "Record my thoughts" starts an ordinary recording that
// is named Thoughts, skips the consent reminder (it is just you) and speaker detection, is filed
// in a Thoughts folder, and is summarized with the "Prompt from my thoughts" template.

import { invoke } from '@tauri-apps/api/core';

export const THOUGHTS = 'thoughts';
export const THOUGHT_TEMPLATE = 'thought_prompt';
const NEXT = 'quire:next-recording-kind';
const CURRENT = 'quire:current-recording-kind';
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

type Store = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
const session = (): Store | null => (typeof window === 'undefined' ? null : window.sessionStorage);

/** "Thoughts, 3 Nov 2026 14:05": a default name, so automatic naming replaces it. */
export function defaultThoughtsTitle(now: Date = new Date()): string {
  const two = (n: number) => String(n).padStart(2, '0');
  return `Thoughts, ${now.getDate()} ${MONTHS[now.getMonth()]} ${now.getFullYear()} ${two(now.getHours())}:${two(now.getMinutes())}`;
}

/** The next recording, if it starts within a few seconds, is thoughts. */
export function requestThoughts(store: Store | null = session(), now = Date.now()): void {
  store?.setItem(NEXT, String(now));
}

/** Called as a recording starts: whether it is thoughts. Only a request made just before counts. */
export function beginRecordingKind(store: Store | null = session(), now = Date.now()): string | null {
  const asked = Number(store?.getItem(NEXT) ?? 0);
  store?.removeItem(NEXT);
  const kind = asked > 0 && now - asked < 10_000 ? THOUGHTS : null;
  if (kind) store?.setItem(CURRENT, kind);
  else store?.removeItem(CURRENT);
  return kind;
}

export function currentRecordingKind(store: Store | null = session()): string | null {
  return store?.getItem(CURRENT) ?? null;
}

/** Called once the recording is saved: the kind it had, then forgets it. */
export function endRecordingKind(store: Store | null = session()): string | null {
  const kind = currentRecordingKind(store);
  store?.removeItem(CURRENT);
  return kind;
}

export const markThoughts = (meetingId: string) => invoke<void>('api_mark_thoughts', { meetingId });
export const meetingKind = (meetingId: string) => invoke<string | null>('api_meeting_kind', { meetingId });
