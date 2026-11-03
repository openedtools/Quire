import { describe, expect, test } from 'bun:test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
// @ts-ignore plain ES module without types
import { findPersonalData, inScope, loadDenyList } from '../../scripts/lint/privacy.mjs';

// Real-looking samples are built from parts, so this file never trips the check it tests.
const AT = '@';
const SAMPLE_USER = 'jane' + 'doe';

const kinds = (text: string, deny: RegExp[] = [], opts = {}) =>
  findPersonalData(text, deny, opts).map((h: { kind: string }) => h.kind);

describe('findPersonalData', () => {
  test('finds real-looking emails and home folders', () => {
    expect(kinds(`contact jane.doe${AT}mail.test.co`)).toEqual(['email']);
    expect(kinds(`cd /Users/${SAMPLE_USER}/projects`)).toEqual(['home folder']);
    expect(kinds(`C:\\Users\\${SAMPLE_USER}\\AppData`)).toEqual(['home folder']);
    expect(kinds(`/home/${SAMPLE_USER}/.cargo`)).toEqual(['home folder']);
  });

  test('placeholders, no-reply addresses and image names pass', () => {
    expect(kinds('someone@example.com, 1+octo@users.noreply.github.com, noreply@anthropic.com')).toEqual([]);
    expect(kinds('/Users/me/Recordings and C:\\Users\\YourName and /home/runner/work')).toEqual([]);
    expect(kinds('icons/128x128@2x.png')).toEqual([]);
  });

  test('private terms from the deny list are found, except in applied migrations', () => {
    const deny = [/\bSecret Project\b/];
    expect(kinds('notes about Secret Project here', deny)).toEqual(['private term']);
    expect(kinds('-- Secret Project', deny, { migration: true })).toEqual([]);
    // The report never repeats the term itself
    expect(JSON.stringify(findPersonalData('Secret Project', deny))).not.toContain('Secret');
  });

  test('the deny list is read from a file outside the repository, and may be missing', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'deny-'));
    const file = path.join(dir, 'deny.txt');
    writeFileSync(file, '# comment\n\\bAlpha\\b\n\nBeta\n');
    expect(loadDenyList(file).map((r: RegExp) => r.source)).toEqual(['\\bAlpha\\b', 'Beta']);
    expect(loadDenyList(path.join(dir, 'missing.txt'))).toEqual([]);
  });

  test('binary files and lockfiles are out of scope', () => {
    expect(inScope('README.md')).toBe(true);
    expect(inScope('brand/source/mark.webp')).toBe(false);
    expect(inScope('frontend/pnpm-lock.yaml')).toBe(false);
  });
});
