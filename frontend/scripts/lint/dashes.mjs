// Fails when a tracked text file contains an em dash (U+2014), except for the files
// listed in dashes.allow.json (a ratchet: counts may only go down, and the list is
// expected to become empty). Usage: node scripts/lint/dashes.mjs [--write-allow]
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMain, repoRoot, trackedFiles } from './lib.mjs';

const EM_DASH = String.fromCharCode(0x2014);
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ALLOW_FILE = path.join(HERE, 'dashes.allow.json');
const BINARY_EXT = new Set([
  '.png', '.jpg', '.jpeg', '.gif', '.webp', '.ico', '.icns', '.bmp', '.mp4', '.mov', '.woff', '.woff2',
  '.ttf', '.otf', '.pdf', '.zip', '.gz', '.db', '.sqlite', '.onnx', '.gguf', '.bin', '.wav', '.m4a', '.mp3',
]);

export function countEmDashes(text) {
  let n = 0;
  for (const ch of text) if (ch === EM_DASH) n += 1;
  return n;
}

export function isProbablyText(file, buf) {
  if (BINARY_EXT.has(path.extname(file).toLowerCase())) return false;
  const head = buf.subarray(0, 8000);
  return !head.includes(0);
}

/** counts: Map(path -> em dash count). allow: { path: maxCount }. Returns failure messages. */
export function evaluateDashes(counts, allow) {
  const failures = [];
  for (const [file, n] of counts) {
    if (n === 0) continue;
    const allowed = allow[file];
    if (allowed === undefined) failures.push(`${file}: ${n} em dash(es) found, none allowed`);
    else if (n > allowed) failures.push(`${file}: ${n} em dashes found, only ${allowed} allowed`);
  }
  for (const file of Object.keys(allow)) {
    if ((counts.get(file) ?? 0) === 0) failures.push(`${file}: allowlist entry is stale (no em dashes left), remove it from dashes.allow.json`);
  }
  return failures;
}

export function scan(root) {
  const counts = new Map();
  for (const file of trackedFiles(root)) {
    let buf;
    try {
      buf = readFileSync(path.join(root, file));
    } catch {
      continue; // deleted in the working tree
    }
    if (!isProbablyText(file, buf)) continue;
    counts.set(file, countEmDashes(buf.toString('utf8')));
  }
  return counts;
}

function main() {
  const root = repoRoot();
  const counts = scan(root);
  if (process.argv.includes('--write-allow')) {
    const allow = {};
    for (const [file, n] of [...counts].sort(([a], [b]) => a.localeCompare(b))) if (n > 0) allow[file] = n;
    writeFileSync(ALLOW_FILE, JSON.stringify(allow, null, 2) + '\n');
    console.log(`dashes: wrote ${Object.keys(allow).length} allowance(s) to dashes.allow.json`);
    return;
  }
  const allow = existsSync(ALLOW_FILE) ? JSON.parse(readFileSync(ALLOW_FILE, 'utf8')) : {};
  const failures = evaluateDashes(counts, allow);
  if (failures.length) {
    console.error('dashes: FAILED');
    for (const f of failures) console.error('  ' + f);
    process.exit(1);
  }
  const allowed = Object.values(allow).reduce((a, b) => a + b, 0);
  console.log(`dashes: ok (${allowed} allowed occurrence(s) in ${Object.keys(allow).length} file(s))`);
}

if (isMain(import.meta.url)) main();
