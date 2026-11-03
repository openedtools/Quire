import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { DEFAULT_TITLE_FORMAT, formatTitle } from '../../src/lib/naming';

// The shared cases the desktop backend and the iPhone app also test (plan Phase L)
const fixture = JSON.parse(readFileSync(join(import.meta.dir, '../../../docs/vault-fixtures/naming.json'), 'utf8'));

describe('Name format preview', () => {
  test('follows the shared cases', () => {
    const d = fixture.date;
    const start = new Date(d.year, d.month - 1, d.day, d.hour, d.minute);
    expect(DEFAULT_TITLE_FORMAT).toBe(fixture.defaultPattern);
    for (const c of fixture.formats) {
      expect(formatTitle(c.pattern, { start, topic: c.topic, folder: c.folder })).toBe(c.title);
    }
  });
});
