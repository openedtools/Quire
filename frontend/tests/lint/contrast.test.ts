import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
// @ts-ignore plain ES module without types
import { contrast, evaluateContrast, parseTokens } from '../../scripts/lint/contrast.mjs';

type Tokens = Record<'light' | 'dark', Record<string, number[]>>;

const STYLES = path.resolve(import.meta.dir, '../../src/styles');
const tokens: Tokens = parseTokens(readFileSync(path.join(STYLES, 'tokens.css'), 'utf8'));
const ratio = (theme: 'light' | 'dark', fg: string, bg: string) => Number(contrast(tokens[theme][fg], tokens[theme][bg]).toFixed(2));

describe('contrast', () => {
  test('black on white is 21 and a colour on itself is 1', () => {
    expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrast([120, 80, 40], [120, 80, 40])).toBeCloseTo(1, 5);
  });

  test('pins the ratios stated in the spec (section 3.2)', () => {
    expect(ratio('light', 'ink', 'paper')).toBe(13.89);
    expect(ratio('light', 'accent-strong', 'paper')).toBe(5.11);
    expect(ratio('light', 'ink-muted', 'paper')).toBe(5.62);
    expect(ratio('light', 'accent', 'paper')).toBe(3.77);
    expect(ratio('light', 'line-strong', 'surface')).toBe(3.39);
    expect(ratio('dark', 'ink', 'paper')).toBe(14.05);
    expect(ratio('dark', 'accent-strong', 'surface')).toBe(5.78);
  });
});

describe('parseTokens', () => {
  test('dark inherits any token it does not redefine', () => {
    const t: Tokens = parseTokens(':root {\n  --a: 1 2 3;\n  --b: 4 5 6;\n}\n.dark {\n  --a: 7 8 9;\n}\n');
    expect(t.light.a).toEqual([1, 2, 3]);
    expect(t.dark).toEqual({ a: [7, 8, 9], b: [4, 5, 6] });
  });
});

describe('evaluateContrast', () => {
  const t = { light: { fg: [120, 120, 120], bg: [255, 255, 255] }, dark: { fg: [120, 120, 120], bg: [255, 255, 255] } };
  const thresholds = { text: 4.5, border: 3 };

  test('fails a text pair below 4.5 but passes the same pair as a border', () => {
    expect(evaluateContrast(t, { thresholds, pairs: [{ fg: 'fg', bg: 'bg', role: 'text' }] }).failures).toHaveLength(2);
    expect(evaluateContrast(t, { thresholds, pairs: [{ fg: 'fg', bg: 'bg', role: 'border' }] }).failures).toHaveLength(0);
  });

  test('a pair limited to one theme is checked only there', () => {
    const r = evaluateContrast(t, { thresholds, pairs: [{ fg: 'fg', bg: 'bg', role: 'text', themes: ['dark'] }] });
    expect(r.rows).toHaveLength(1);
  });

  test('unknown tokens and roles are failures, not silent passes', () => {
    expect(evaluateContrast(t, { thresholds, pairs: [{ fg: 'nope', bg: 'bg', role: 'text' }] }).failures).toHaveLength(2);
    expect(evaluateContrast(t, { thresholds, pairs: [{ fg: 'fg', bg: 'bg', role: 'shout' }] }).failures).toHaveLength(1);
  });

  test('the real token file passes', () => {
    const spec = JSON.parse(readFileSync(path.join(STYLES, 'tokens.contrast.json'), 'utf8'));
    expect(evaluateContrast(tokens, spec).failures).toEqual([]);
  });
});
