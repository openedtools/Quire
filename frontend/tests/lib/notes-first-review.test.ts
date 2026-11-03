import { describe, expect, test } from 'bun:test';
import {
  canStartRecording,
  cancelScheduledDiscard,
  claimForRecording,
  clearRecordTarget,
  createNotesMeeting,
  discardIfUntouched,
  recordingTargetId,
  scheduleDiscard,
  type KeyValueStore,
} from '../../src/lib/notesFirst';

function memoryStore(): KeyValueStore {
  const data = new Map<string, string>();
  return {
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
    removeItem: (k) => void data.delete(k),
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function recorder() {
  const calls: [string, unknown][] = [];
  const invoke = async (cmd: string, args?: Record<string, unknown>) => {
    calls.push([cmd, args]);
    return (cmd === 'api_create_meeting' ? 'meeting-new' : true) as never;
  };
  return { calls, invoke };
}

describe('Notes-first review fixes', () => {
  test('a recording can start only when nothing is starting, running or still saving', () => {
    for (const s of ['idle', 'completed', 'error'] as const) expect(canStartRecording(s)).toBe(true);
    for (const s of ['starting', 'recording', 'stopping', 'processing', 'saving'] as const) expect(canStartRecording(s)).toBe(false);
  });

  test('the target comes from the recording session, not from the window', async () => {
    const meta = new Map([['session-1', { targetMeetingId: 'meeting-a' }], ['session-2', {}]]);
    const get = async (id: string) => meta.get(id) ?? null;
    expect(await recordingTargetId('session-1', get)).toBe('meeting-a');
    expect(await recordingTargetId('session-2', get)).toBeNull();
    expect(await recordingTargetId('session-missing', get)).toBeNull();
    expect(await recordingTargetId(null, get)).toBeNull();
  });

  test('a meeting claimed for recording is never discarded, even if the recording fails to start', async () => {
    const store = memoryStore();
    const { calls, invoke } = recorder();
    const meeting = await createNotesMeeting({ store, invoke });
    claimForRecording(meeting, store);
    clearRecordTarget(store);
    expect(await discardIfUntouched(meeting.id, { store, invoke })).toBe(false);
    expect(calls.filter(([c]) => c === 'api_discard_empty_meeting')).toEqual([]);
  });

  test('coming back to a meeting cancels its pending discard', async () => {
    const store = memoryStore();
    const { calls, invoke } = recorder();
    const meeting = await createNotesMeeting({ store, invoke });
    scheduleDiscard(meeting.id, { store, invoke, delayMs: 20 });
    cancelScheduledDiscard(meeting.id);
    await sleep(60);
    expect(calls.filter(([c]) => c === 'api_discard_empty_meeting')).toEqual([]);
    // Leaving again still tidies it up
    scheduleDiscard(meeting.id, { store, invoke, delayMs: 20 });
    await sleep(60);
    expect(calls.filter(([c]) => c === 'api_discard_empty_meeting').length).toBe(1);
  });
});
