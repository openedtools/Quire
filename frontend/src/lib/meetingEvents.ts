/**
 * Window events between the Ask box and the panes beside it, which do not share a parent that
 * could pass callbacks down: jump the transcript to a time, and append text to My notes.
 */

const JUMP = 'quire:jump-to-time';
const APPEND = 'quire:append-to-notes';

export function jumpToTime(seconds: number): void {
  window.dispatchEvent(new CustomEvent<number>(JUMP, { detail: seconds }));
}

export function onJumpToTime(handler: (seconds: number) => void): () => void {
  const listener = (e: Event) => handler((e as CustomEvent<number>).detail);
  window.addEventListener(JUMP, listener);
  return () => window.removeEventListener(JUMP, listener);
}

export function appendToNotes(markdown: string): void {
  window.dispatchEvent(new CustomEvent<string>(APPEND, { detail: markdown }));
}

export function onAppendToNotes(handler: (markdown: string) => void): () => void {
  const listener = (e: Event) => handler((e as CustomEvent<string>).detail);
  window.addEventListener(APPEND, listener);
  return () => window.removeEventListener(APPEND, listener);
}
