import { describe, expect, test } from 'bun:test';
import {
  clearRecordTarget,
  createNotesMeeting,
  defaultMeetingTitle,
  discardIfUntouched,
  getRecordTarget,
  setRecordTarget,
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

describe('Notes-first meetings', () => {
  test('the default title names the day and time', () => {
    expect(defaultMeetingTitle(new Date(2026, 10, 2, 14, 5))).toBe('Meeting, 2 Nov 2026 14:05');
    expect(defaultMeetingTitle(new Date(2026, 0, 9, 8, 0))).toBe('Meeting, 9 Jan 2026 08:00');
  });

  test('the record target is kept until cleared, and junk reads as none', () => {
    const store = memoryStore();
    expect(getRecordTarget(store)).toBeNull();
    setRecordTarget({ id: 'meeting-1', title: 'Planning' }, store);
    expect(getRecordTarget(store)).toEqual({ id: 'meeting-1', title: 'Planning' });
    clearRecordTarget(store);
    expect(getRecordTarget(store)).toBeNull();
    store.setItem('quire_record_into_meeting', '{not json');
    expect(getRecordTarget(store)).toBeNull();
  });

  test('a new meeting is created with the default title and remembered as fresh', async () => {
    const store = memoryStore();
    const calls: unknown[] = [];
    const meeting = await createNotesMeeting({
      store,
      now: () => new Date(2026, 10, 2, 14, 5),
      invoke: async (cmd, args) => {
        calls.push([cmd, args]);
        return 'meeting-new' as never;
      },
    });
    expect(meeting).toEqual({ id: 'meeting-new', title: 'Meeting, 2 Nov 2026 14:05' });
    expect(calls).toEqual([['api_create_meeting', { title: 'Meeting, 2 Nov 2026 14:05' }]]);
  });

  test('leaving a fresh meeting asks the app to discard it once, with its default title', async () => {
    const store = memoryStore();
    const calls: unknown[] = [];
    const invoke = async (cmd: string, args?: Record<string, unknown>) => {
      calls.push([cmd, args]);
      return (cmd === 'api_create_meeting' ? 'meeting-new' : true) as never;
    };
    await createNotesMeeting({ store, now: () => new Date(2026, 10, 2, 14, 5), invoke });
    expect(await discardIfUntouched('meeting-new', { store, invoke })).toBe(true);
    expect(calls[1]).toEqual(['api_discard_empty_meeting', { meetingId: 'meeting-new', defaultTitle: 'Meeting, 2 Nov 2026 14:05' }]);
    // Only once: the meeting is no longer fresh
    expect(await discardIfUntouched('meeting-new', { store, invoke })).toBe(false);
    expect(calls.length).toBe(2);
  });

  test('a meeting that was not made by New meeting is never discarded', async () => {
    const calls: unknown[] = [];
    const invoke = async (cmd: string, args?: Record<string, unknown>) => {
      calls.push([cmd, args]);
      return true as never;
    };
    expect(await discardIfUntouched('meeting-recorded', { store: memoryStore(), invoke })).toBe(false);
    expect(calls).toEqual([]);
  });

  test('a fresh meeting being recorded into is not discarded when the page is left', async () => {
    const store = memoryStore();
    const calls: unknown[] = [];
    const invoke = async (cmd: string, args?: Record<string, unknown>) => {
      calls.push([cmd, args]);
      return (cmd === 'api_create_meeting' ? 'meeting-new' : true) as never;
    };
    const meeting = await createNotesMeeting({ store, now: () => new Date(2026, 10, 2, 14, 5), invoke });
    setRecordTarget(meeting, store);
    expect(await discardIfUntouched('meeting-new', { store, invoke })).toBe(false);
    expect(calls.length).toBe(1);
  });
});
