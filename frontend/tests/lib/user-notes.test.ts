import { describe, expect, mock, test } from 'bun:test';
import { attachLiveNotes, type AttachDeps } from '../../src/lib/userNotes';

function deps(live: { blocks: string; markdown: string } | null, saveFails = false) {
  const store = new Map<string, unknown>();
  if (live) store.set('session-1', { meetingId: 'session-1', updatedAt: 1, ...live });
  const save = mock(async () => {
    if (saveFails) throw new Error('database is busy');
  });
  const d: AttachDeps = {
    getLiveNotes: async (id) => (store.get(id) as never) ?? null,
    deleteLiveNotes: async (id) => {
      store.delete(id);
    },
    save,
  };
  return { d, store, save };
}

describe('attachLiveNotes', () => {
  test('notes typed during a recording move to the saved meeting', async () => {
    const { d, store, save } = deps({ blocks: '[1]', markdown: 'Budget is fixed' });
    expect(await attachLiveNotes('session-1', 'meeting-9', d)).toBe(true);
    expect(save).toHaveBeenCalledWith('meeting-9', '[1]', 'Budget is fixed');
    expect(store.has('session-1')).toBe(false);
  });

  test('no live notes means nothing is saved', async () => {
    const { d, save } = deps(null);
    expect(await attachLiveNotes('session-1', 'meeting-9', d)).toBe(false);
    expect(save).not.toHaveBeenCalled();
  });

  test('blank live notes are discarded without a save', async () => {
    const { d, store, save } = deps({ blocks: '[]', markdown: '  \n' });
    expect(await attachLiveNotes('session-1', 'meeting-9', d)).toBe(false);
    expect(save).not.toHaveBeenCalled();
    expect(store.has('session-1')).toBe(false);
  });

  test('a failed save keeps the live copy for recovery', async () => {
    const { d, store } = deps({ blocks: '[1]', markdown: 'keep me' }, true);
    await expect(attachLiveNotes('session-1', 'meeting-9', d)).rejects.toThrow('database is busy');
    expect(store.has('session-1')).toBe(true);
  });
});
