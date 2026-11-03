/**
 * Notes-first meetings (spec F-NM): "New meeting" creates the meeting at once, and a recording
 * started from it is saved into it. Both facts live in sessionStorage, so they last while the
 * window is open and never outlive it.
 */
import { invoke as tauriInvoke } from '@tauri-apps/api/core';

export interface KeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface RecordTarget {
  id: string;
  title: string;
}

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

interface Deps {
  store?: KeyValueStore;
  invoke?: Invoke;
  now?: () => Date;
}

const TARGET_KEY = 'quire_record_into_meeting';
/** Meetings made by New meeting in this window, with the title they were given. */
const FRESH_KEY = 'quire_fresh_meetings';
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function sessionStore(): KeyValueStore {
  return sessionStorage;
}

function readJson<T>(store: KeyValueStore, key: string): T | null {
  try {
    const raw = store.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

/** "Meeting, 2 Nov 2026 14:05" (plan PD5). */
export function defaultMeetingTitle(now: Date = new Date()): string {
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `Meeting, ${now.getDate()} ${MONTHS[now.getMonth()]} ${now.getFullYear()} ${hh}:${mm}`;
}

/** The meeting the next recording is saved into (F-NM-3). */
export function setRecordTarget(target: RecordTarget, store: KeyValueStore = sessionStore()): void {
  store.setItem(TARGET_KEY, JSON.stringify(target));
}

export function getRecordTarget(store: KeyValueStore = sessionStore()): RecordTarget | null {
  const target = readJson<RecordTarget>(store, TARGET_KEY);
  return target && typeof target.id === 'string' && typeof target.title === 'string' ? target : null;
}

export function clearRecordTarget(store: KeyValueStore = sessionStore()): void {
  store.removeItem(TARGET_KEY);
}

/** Create a meeting with no recording and remember it as fresh (F-NM-1). */
export async function createNotesMeeting(deps: Deps = {}): Promise<RecordTarget> {
  const store = deps.store ?? sessionStore();
  const invoke = deps.invoke ?? tauriInvoke;
  const title = defaultMeetingTitle(deps.now?.() ?? new Date());
  const id = await invoke<string>('api_create_meeting', { title });
  const fresh = readJson<Record<string, string>>(store, FRESH_KEY) ?? {};
  fresh[id] = title;
  store.setItem(FRESH_KEY, JSON.stringify(fresh));
  return { id, title };
}

export type RecordingStatusName = 'idle' | 'starting' | 'recording' | 'stopping' | 'processing' | 'saving' | 'completed' | 'error';

/** True when no recording is starting, running or still being saved. */
export function canStartRecording(status: RecordingStatusName): boolean {
  return status === 'idle' || status === 'completed' || status === 'error';
}

/** Set the target for the next recording and stop treating the meeting as fresh. */
export function claimForRecording(target: RecordTarget, store: KeyValueStore = sessionStore()): void {
  setRecordTarget(target, store);
  // Kept even if the recording then fails to start: the person chose this meeting
  const fresh = readJson<Record<string, string>>(store, FRESH_KEY) ?? {};
  delete fresh[target.id];
  store.setItem(FRESH_KEY, JSON.stringify(fresh));
}

/**
 * The meeting a recording session belongs to, from its own recovery record. The window's target
 * can change while an earlier recording is still being saved, so the save never reads it.
 */
export async function recordingTargetId(
  sessionId: string | null,
  getMetadata: (id: string) => Promise<{ targetMeetingId?: string } | null | undefined>,
): Promise<string | null> {
  if (!sessionId) return null;
  return (await getMetadata(sessionId))?.targetMeetingId ?? null;
}

const pendingDiscards = new Map<string, ReturnType<typeof setTimeout>>();

/**
 * Check a left meeting after a wait, so notes typed just before leaving finish saving. Coming
 * back to the meeting first cancels the check (cancelScheduledDiscard).
 */
export function scheduleDiscard(meetingId: string, options: Deps & { delayMs?: number; onRemoved?: () => void } = {}): void {
  cancelScheduledDiscard(meetingId);
  const timer = setTimeout(() => {
    pendingDiscards.delete(meetingId);
    discardIfUntouched(meetingId, options)
      .then((removed) => { if (removed) options.onRemoved?.(); })
      .catch((error) => console.error('Failed to tidy up an unused meeting:', error));
  }, options.delayMs ?? 2000);
  pendingDiscards.set(meetingId, timer);
}

export function cancelScheduledDiscard(meetingId: string): void {
  const timer = pendingDiscards.get(meetingId);
  if (timer) clearTimeout(timer);
  pendingDiscards.delete(meetingId);
}

/**
 * Called when a meeting page is left. A fresh meeting with nothing in it is removed (F-NM-5);
 * the app makes the final check, so a meeting with notes, a recording or a new title stays.
 * Returns true when the meeting was removed.
 */
export async function discardIfUntouched(meetingId: string, deps: Deps = {}): Promise<boolean> {
  const store = deps.store ?? sessionStore();
  const invoke = deps.invoke ?? tauriInvoke;
  if (getRecordTarget(store)?.id === meetingId) return false;
  const fresh = readJson<Record<string, string>>(store, FRESH_KEY) ?? {};
  const defaultTitle = fresh[meetingId];
  if (defaultTitle === undefined) return false;
  delete fresh[meetingId];
  store.setItem(FRESH_KEY, JSON.stringify(fresh));
  return invoke<boolean>('api_discard_empty_meeting', { meetingId, defaultTitle });
}
