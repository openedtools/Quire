import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { beginRecordingKind, defaultThoughtsTitle, endRecordingKind, requestThoughts, THOUGHTS } from '../../src/lib/thoughts';

const fixture = JSON.parse(readFileSync(join(import.meta.dir, '../../../docs/vault-fixtures/thoughts.json'), 'utf8'));

function memory() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) };
}

describe('Thought mode', () => {
  test('the default name has the shared shape', () => {
    const title = defaultThoughtsTitle(new Date(2026, 10, 3, 14, 5));
    expect(title).toBe('Thoughts, 3 Nov 2026 14:05');
    expect(fixture.defaultTitles.yes).toContain(title);
    expect(fixture.kindValue).toBe(THOUGHTS);
  });

  test('only a request made just before the start counts, once', () => {
    const store = memory();
    requestThoughts(store, 1000);
    expect(beginRecordingKind(store, 3000)).toBe(THOUGHTS);
    expect(endRecordingKind(store)).toBe(THOUGHTS);
    expect(endRecordingKind(store)).toBeNull();
    expect(beginRecordingKind(store, 4000)).toBeNull();
    requestThoughts(store, 1000);
    expect(beginRecordingKind(store, 60_000)).toBeNull();
  });
});
