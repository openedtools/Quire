import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { evaluateMentions, findMentions, globToRegExp, inScope, parseAllowlist } from '../../scripts/lint/brand.mjs';

describe('findMentions', () => {
  test('finds the earlier product name in any case and reports line numbers', () => {
    const hits = findMentions('one Meetily\nclean\nMEETILY and meetily');
    expect(hits.map((h: { line: number }) => h.line)).toEqual([1, 3]);
  });
  test('finds nothing in clean text', () => {
    expect(findMentions('nothing here\nat all')).toEqual([]);
  });
});

describe('parseAllowlist', () => {
  test('reads glob and reason pairs and skips comments and blank lines', () => {
    const list = parseAllowlist('# a comment\n\nfoo/** | because\nbar.ts | other reason\n');
    expect(list).toEqual([
      { glob: 'foo/**', reason: 'because' },
      { glob: 'bar.ts', reason: 'other reason' },
    ]);
  });
  test('an entry without a reason is rejected', () => {
    expect(() => parseAllowlist('foo/**\n')).toThrow(/reason/);
    expect(() => parseAllowlist('foo/** |   \n')).toThrow(/reason/);
  });
});

describe('globToRegExp', () => {
  test('double star crosses folders, single star does not', () => {
    expect(globToRegExp('a/**').test('a/b/c.ts')).toBe(true);
    expect(globToRegExp('a/*.ts').test('a/x.ts')).toBe(true);
    expect(globToRegExp('a/*.ts').test('a/b/x.ts')).toBe(false);
    expect(globToRegExp('a/x.ts').test('a/x.tsx')).toBe(false);
  });
  test('dots are literal', () => {
    expect(globToRegExp('a.ts').test('aXts')).toBe(false);
  });
});

describe('inScope', () => {
  test('covers product source, build scripts and configuration only', () => {
    expect(inScope('frontend/src/components/About.tsx')).toBe(true);
    expect(inScope('frontend/src-tauri/src/lib.rs')).toBe(true);
    expect(inScope('frontend/src-tauri/Cargo.toml')).toBe(true);
    expect(inScope('README.md')).toBe(false);
    expect(inScope('docs/architecture.md')).toBe(false);
    expect(inScope('backend/app/main.py')).toBe(false);
    expect(inScope('.github/workflows/build.yml')).toBe(false);
  });
});

describe('evaluateMentions', () => {
  const files = new Map([
    ['frontend/src/a.ts', [{ line: 3, text: 'Meetily' }]],
    ['frontend/src/b.ts', []],
    ['frontend/src/legacy/x.ts', [{ line: 1, text: 'meetily' }]],
  ]);
  test('a mention outside the allowlist fails and names file and line', () => {
    const failures = evaluateMentions(files, [{ glob: 'frontend/src/legacy/**', reason: 'r' }]);
    expect(failures).toHaveLength(1);
    expect(failures[0]).toContain('frontend/src/a.ts:3');
  });
  test('allowed files pass', () => {
    const failures = evaluateMentions(files, [
      { glob: 'frontend/src/legacy/**', reason: 'r' },
      { glob: 'frontend/src/a.ts', reason: 'r' },
    ]);
    expect(failures).toEqual([]);
  });
  test('an allowance that matches no mention is stale and fails', () => {
    const failures = evaluateMentions(files, [
      { glob: 'frontend/src/legacy/**', reason: 'r' },
      { glob: 'frontend/src/a.ts', reason: 'r' },
      { glob: 'frontend/src/b.ts', reason: 'no longer needed' },
    ]);
    expect(failures).toHaveLength(1);
    expect(failures[0]).toContain('frontend/src/b.ts');
  });
});
