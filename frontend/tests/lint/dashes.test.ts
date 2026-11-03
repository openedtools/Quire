import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { countEmDashes, evaluateDashes, isProbablyText } from '../../scripts/lint/dashes.mjs';

const EM = String.fromCharCode(0x2014);
const EN = String.fromCharCode(0x2013);

describe('countEmDashes', () => {
  test('counts every em dash and ignores en dashes and hyphens', () => {
    expect(countEmDashes(`a${EM}b${EM}c`)).toBe(2);
    expect(countEmDashes(`a${EN}b - c -- d`)).toBe(0);
    expect(countEmDashes('')).toBe(0);
  });
});

describe('evaluateDashes', () => {
  test('a file with em dashes that is not allowed fails and is named', () => {
    const failures = evaluateDashes(new Map([['src/new.ts', 1]]), {});
    expect(failures).toHaveLength(1);
    expect(failures[0]).toContain('src/new.ts');
  });

  test('an allowed file passes at or below its allowance', () => {
    expect(evaluateDashes(new Map([['a.rs', 3]]), { 'a.rs': 3 })).toEqual([]);
    expect(evaluateDashes(new Map([['a.rs', 2]]), { 'a.rs': 3 })).toEqual([]);
  });

  test('an allowed file that gains em dashes fails', () => {
    const failures = evaluateDashes(new Map([['a.rs', 4]]), { 'a.rs': 3 });
    expect(failures).toHaveLength(1);
    expect(failures[0]).toContain('a.rs');
  });

  test('files with no em dashes never fail', () => {
    expect(evaluateDashes(new Map([['clean.ts', 0]]), {})).toEqual([]);
  });

  test('a stale allowance (file now clean or gone) must be removed', () => {
    const failures = evaluateDashes(new Map([['a.rs', 0]]), { 'a.rs': 3, 'gone.rs': 1 });
    expect(failures.some((f: string) => f.includes('a.rs'))).toBe(true);
    expect(failures.some((f: string) => f.includes('gone.rs'))).toBe(true);
  });
});

describe('isProbablyText', () => {
  test('skips known binary extensions and files with NUL bytes', () => {
    expect(isProbablyText('logo.png', Buffer.from('x'))).toBe(false);
    expect(isProbablyText('notes.md', Buffer.from('hello'))).toBe(true);
    expect(isProbablyText('weird.dat', Buffer.from([0x41, 0x00, 0x42]))).toBe(false);
  });
});
