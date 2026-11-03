import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { findRawColours, inScope } from '../../scripts/lint/colors.mjs';
// @ts-ignore plain ES module without types
import { mapColour, rewrite } from '../../scripts/codemod/palette.mjs';

describe('findRawColours', () => {
  test('finds palette classes with variants and opacity, and hex colours', () => {
    const hits = findRawColours('a "bg-gray-100 hover:text-blue-700"\nb "bg-black/50" style={{ color: "#3b82f6" }}');
    expect(hits.map((h: { text: string }) => h.text)).toEqual(['bg-gray-100', 'hover:text-blue-700', 'bg-black/50', '#3b82f6']);
    expect(hits.map((h: { line: number }) => h.line)).toEqual([1, 1, 2, 2]);
  });

  test('semantic token classes and look-alike words pass', () => {
    expect(findRawColours('bg-surface text-ink-muted border-line-strong bg-accent-tint text-on-accent bg-speaker-1')).toEqual([]);
    expect(findRawColours('const grayed = "graybox"; whitespace-nowrap blacklist')).toEqual([]);
  });

  test('only UI source is in scope, and tokens.css is the one exception', () => {
    expect(inScope('frontend/src/components/A.tsx')).toBe(true);
    expect(inScope('frontend/src/app/globals.css')).toBe(true);
    expect(inScope('frontend/src/styles/tokens.css')).toBe(false);
    expect(inScope('frontend/tests/ui/mock-tauri.js')).toBe(false);
  });
});

describe('palette codemod', () => {
  test('maps the plan table', () => {
    expect(mapColour('bg', 'gray', '50')).toBe('surface');
    expect(mapColour('bg', 'white', undefined)).toBe('paper');
    expect(mapColour('border', 'gray', '200')).toBe('line');
    expect(mapColour('border', 'gray', '300')).toBe('line-strong');
    expect(mapColour('text', 'gray', '900')).toBe('ink');
    expect(mapColour('text', 'gray', '500')).toBe('ink-muted');
    expect(mapColour('bg', 'blue', '600')).toBe('accent-strong');
    expect(mapColour('bg', 'blue', '50')).toBe('accent-tint');
    expect(mapColour('text', 'red', '700')).toBe('danger');
    expect(mapColour('bg', 'amber', '50')).toBe('warning-tint');
    expect(mapColour('bg', 'black', undefined)).toBe('scrim');
  });

  test('hover shades become hover tokens, not the same colour', () => {
    expect(rewrite('bg-gray-50 hover:bg-gray-100').text).toBe('bg-surface hover:bg-surface-hover');
    expect(rewrite('bg-red-600 hover:bg-red-700').text).toBe('bg-danger hover:bg-danger/90');
    expect(rewrite('bg-blue-600 hover:bg-blue-700').text).toBe('bg-accent-strong hover:bg-accent-strong-hover');
  });

  test('keeps opacity and removes drop shadows', () => {
    expect(rewrite('bg-white/80 shadow-md p-2').text).toBe('bg-paper/80 p-2');
    expect(rewrite('shadow-none').text).toBe('shadow-none');
  });
});
