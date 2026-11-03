import { describe, expect, test } from 'bun:test';
import { applyFix, clock, kindOf, repeatedKinds, type Suggestion } from '../../src/lib/transcriptEdits';

const s = (lineId: string, find: string, replace: string): Suggestion => ({ lineId, start: 0, lineText: '', find, replace, reason: '' });

describe('Transcript check', () => {
  test('a fix replaces the first occurrence only', () => {
    expect(applyFix('The guarden club and the guarden gate.', 'guarden', 'garden')).toBe('The garden club and the guarden gate.');
    expect(applyFix('Nothing here.', 'guarden', 'garden')).toBe('Nothing here.');
  });

  test('repeated changes are offered together', () => {
    const kinds = repeatedKinds([s('a', 'Anna', 'Ana'), s('b', 'Anna', 'Ana'), s('c', 'guarden', 'garden')]);
    expect(kinds.get(kindOf({ find: 'Anna', replace: 'Ana' }))).toBe(2);
    expect(kinds.has(kindOf({ find: 'guarden', replace: 'garden' }))).toBe(false);
  });

  test('times read as on the transcript', () => {
    expect(clock(0)).toBe('[00:00]');
    expect(clock(125.6)).toBe('[02:05]');
  });
});
