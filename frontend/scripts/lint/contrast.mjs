// Checks WCAG contrast for every token pair in src/styles/tokens.contrast.json, in both
// themes, using the values in src/styles/tokens.css (spec F-UI-3, AC-UI-1).
// Usage: node scripts/lint/contrast.mjs [--report]
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMain } from './lib.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const STYLES = path.resolve(HERE, '..', '..', 'src', 'styles');

/**
 * { light: {name: [r,g,b]}, dark: {...} } from the :root and .dark blocks. Dark inherits light.
 * @param {string} css
 * @returns {{ light: Record<string, number[]>, dark: Record<string, number[]> }}
 */
export function parseTokens(css) {
  const block = (selector) => {
    const start = css.indexOf(`${selector} {`);
    if (start === -1) throw new Error(`tokens.css has no ${selector} block`);
    return css.slice(start, css.indexOf('}', start));
  };
  const read = (text) => {
    /** @type {Record<string, number[]>} */
    const out = {};
    for (const m of text.matchAll(/--([a-z0-9-]+):\s*(\d{1,3})\s+(\d{1,3})\s+(\d{1,3})\s*;/g)) {
      out[m[1]] = [Number(m[2]), Number(m[3]), Number(m[4])];
    }
    return out;
  };
  const light = read(block(':root'));
  return { light, dark: { ...light, ...read(block('.dark')) } };
}

function luminance([r, g, b]) {
  const f = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Returns { failures: string[], rows: string[] }. */
export function evaluateContrast(tokens, spec) {
  const failures = [];
  const rows = [];
  for (const pair of spec.pairs) {
    const threshold = spec.thresholds[pair.role];
    if (threshold === undefined) {
      failures.push(`unknown role "${pair.role}" for ${pair.fg} on ${pair.bg}`);
      continue;
    }
    for (const theme of pair.themes ?? ['light', 'dark']) {
      const fg = tokens[theme][pair.fg];
      const bg = tokens[theme][pair.bg];
      if (!fg || !bg) {
        failures.push(`${theme}: ${pair.fg} on ${pair.bg} names a token that tokens.css does not define`);
        continue;
      }
      const ratio = contrast(fg, bg);
      const line = `${theme.padEnd(5)} ${pair.fg} on ${pair.bg}: ${ratio.toFixed(2)} (${pair.role}, needs ${threshold})`;
      rows.push(line);
      if (ratio < threshold) failures.push(line);
    }
  }
  return { failures, rows };
}

if (isMain(import.meta.url)) {
  const tokens = parseTokens(readFileSync(path.join(STYLES, 'tokens.css'), 'utf8'));
  const spec = JSON.parse(readFileSync(path.join(STYLES, 'tokens.contrast.json'), 'utf8'));
  const { failures, rows } = evaluateContrast(tokens, spec);
  if (process.argv.includes('--report')) for (const row of rows) console.log(row);
  if (failures.length) {
    console.error('contrast: FAILED');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`contrast: ok (${rows.length} pair(s) checked in two themes)`);
}
