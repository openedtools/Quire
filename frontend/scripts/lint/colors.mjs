// Fails on raw Tailwind palette classes (gray-600, blue-50, white ...) or hex colours in the
// UI source. Colours come only from src/styles/tokens.css (spec F-UI-1, F-UI-7, AC-UI-2).
// Usage: node scripts/lint/colors.mjs
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isMain, repoRoot, trackedFiles } from './lib.mjs';
import { RAW, findHex } from '../codemod/palette.mjs';

const SCOPE = /^frontend\/src\/.*\.(tsx?|jsx?|css)$/;
const ALLOWED = new Set(['frontend/src/styles/tokens.css']);

/** Returns [{ line, text }] for each raw colour use in one file's text. */
export function findRawColours(text) {
  const hits = [];
  text.split(/\r?\n/).forEach((line, i) => {
    for (const m of line.matchAll(RAW)) hits.push({ line: i + 1, text: m[0] });
    for (const h of findHex(line)) hits.push({ line: i + 1, text: h });
  });
  return hits;
}

export function inScope(file) {
  return SCOPE.test(file) && !ALLOWED.has(file);
}

if (isMain(import.meta.url)) {
  const root = repoRoot();
  const failures = [];
  for (const file of trackedFiles(root).filter(inScope)) {
    for (const hit of findRawColours(readFileSync(path.join(root, file), 'utf8'))) {
      failures.push(`${file}:${hit.line}: ${hit.text}`);
    }
  }
  if (failures.length) {
    console.error('colors: FAILED (use the semantic token classes from tailwind.config.js)');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log('colors: ok (no raw palette classes or hex colours outside tokens.css)');
}
