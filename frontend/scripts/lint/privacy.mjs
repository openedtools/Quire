// Fails when a tracked file holds personal data: an email address that is not a placeholder or a
// no-reply address, a real home-folder path, or a term from the owner's private deny list.
// The deny list lives outside the repository (it is itself personal):
//   ~/.config/quire/privacy-deny.txt   one regular expression per line, # for comments
// Usage: node scripts/lint/privacy.mjs
import { existsSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { isMain, repoRoot, trackedFiles } from './lib.mjs';

const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}/g;
const ALLOWED_EMAIL =
  /@(example\.(com|org|net)|users\.noreply\.github\.com|anthropic\.com)$|^noreply@|^[0-9]+x[0-9]+@[0-9]x\.png$|@[0-9]x\.png$/i;
const HOME = /(?:\/Users\/|\/home\/|[Cc]:\\+Users\\+)([A-Za-z0-9._-]+)/g;
const PLACEHOLDER_USERS = new Set(['me', 'you', 'user', 'username', 'yourname', 'runner', 'example', 'name', 'shared', 'public']);
const BINARY = /\.(png|jpe?g|webp|ico|icns|bmp|gif|m4a|wav|mp3|mp4|onnx|ttf|woff2?|pdf|exe|dmg|zip|gz|xz)$/i;
// Lockfiles hold upstream package metadata; applied migrations must never change (sqlx checksums).
const EXEMPT = /(^|\/)(pnpm-lock\.yaml|Cargo\.lock|package-lock\.json)$/;
const MIGRATION = /^frontend\/src-tauri\/migrations\//;

export const DENY_FILE = path.join(os.homedir(), '.config', 'quire', 'privacy-deny.txt');

/** The owner's private terms, as regular expressions; none when the file is absent. */
export function loadDenyList(file = DENY_FILE) {
  if (!existsSync(file)) return [];
  return readFileSync(file, 'utf8')
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith('#'))
    .map((l) => new RegExp(l));
}

/** Returns [{ line, kind, text }] for each personal item in one file's text. */
export function findPersonalData(text, deny = [], { migration = false } = {}) {
  const hits = [];
  text.split(/\r?\n/).forEach((line, i) => {
    for (const m of line.matchAll(EMAIL)) {
      if (!ALLOWED_EMAIL.test(m[0])) hits.push({ line: i + 1, kind: 'email', text: m[0] });
    }
    for (const m of line.matchAll(HOME)) {
      if (!PLACEHOLDER_USERS.has(m[1].toLowerCase())) hits.push({ line: i + 1, kind: 'home folder', text: m[0] });
    }
    if (!migration) {
      for (const re of deny) {
        const m = line.match(re);
        // the matched text itself is not printed, so the report does not repeat it
        if (m) hits.push({ line: i + 1, kind: 'private term', text: `rule ${re.source.length} chars` });
      }
    }
  });
  return hits;
}

export function inScope(file) {
  return !BINARY.test(file) && !EXEMPT.test(file);
}

if (isMain(import.meta.url)) {
  const root = repoRoot();
  const deny = loadDenyList();
  const failures = [];
  for (const file of trackedFiles(root).filter(inScope)) {
    let text;
    try {
      text = readFileSync(path.join(root, file), 'utf8');
    } catch {
      continue;
    }
    for (const hit of findPersonalData(text, deny, { migration: MIGRATION.test(file) })) {
      failures.push(`${file}:${hit.line}: ${hit.kind}: ${hit.text}`);
    }
  }
  if (failures.length) {
    console.error('privacy: FAILED (personal data must never be committed; remove it or ask the owner)');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`privacy: ok (no personal data; private deny list ${deny.length ? `with ${deny.length} rule(s)` : 'not found on this computer'})`);
}
