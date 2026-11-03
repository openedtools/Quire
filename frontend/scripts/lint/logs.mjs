// Fails when a Rust log call may print meeting content: transcript text, titles, notes, questions,
// answers, names, keys, notification text, or a file or folder path (meeting folders carry titles)
// (spec NF-6, AC-HK-4). A call that only looks suspicious (a segment's sample count, a model
// folder) is accepted with a reviewed `// log-ok: <reason>` comment on the line above it.
// Usage: node scripts/lint/logs.mjs
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isMain, repoRoot, trackedFiles } from './lib.mjs';

const MACRO = /\b(?:log::)?(?:log_)?(info|warn|error|debug|trace|perf_debug|perf_trace|println|eprintln)!\s*\(/g;
const IDENT = /\b[A-Za-z_][A-Za-z0-9_]*\b/g;
const SENSITIVE =
  /(?<![a-z])(text|title|transcript|question|prompt|answer|api_key|apikey|markdown|notes|body|person_name|folder_path|meeting_folder|save_folder|meeting_name|segment|content|response|cleaned_result|final_result|path|folder|display)(?![a-z])/;
const HARMLESS = new Set(['context', 'content_type', 'status']);
const STRING = /"(?:[^"\\]|\\.)*"/g;

/** The source before the first test module, which never runs in the app. */
function appSource(text) {
  const test = text.search(/#\[cfg\(test\)\]\s*(pub(\(crate\))? )?mod /);
  return test < 0 ? text : text.slice(0, test);
}

/** Returns [{ line, names, call }] for each log call that may print something sensitive. */
export function findSensitiveLogs(text) {
  const source = appSource(text);
  const lines = source.split('\n');
  const hits = [];
  for (const m of source.matchAll(MACRO)) {
    let i = m.index + m[0].length;
    let depth = 1;
    while (i < source.length && depth) {
      const c = source[i];
      if (c === '"') {
        // skip a string literal, so brackets inside it do not count
        i += 1;
        while (i < source.length && source[i] !== '"') i += source[i] === '\\' ? 2 : 1;
      } else if (c === '(') depth += 1;
      else if (c === ')') depth -= 1;
      i += 1;
    }
    const call = source.slice(m.index, i);
    const inline = [...call.matchAll(STRING)].flatMap((s) => [...s[0].matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)/g)].map((x) => x[1]));
    const args = call.slice(call.indexOf('(') + 1).replace(STRING, '""');
    const names = [...new Set([...(args.match(IDENT) ?? []), ...inline])]
      .filter((n) => !HARMLESS.has(n) && SENSITIVE.test(n.toLowerCase()))
      .sort();
    if (!names.length) continue;
    const line = source.slice(0, m.index).split('\n').length;
    const above = (lines[line - 2] ?? '').trim();
    if (above.startsWith('// log-ok:')) continue;
    hits.push({ line, names, call: call.replace(/\s+/g, ' ').slice(0, 140) });
  }
  return hits;
}

export function inScope(file) {
  return /^frontend\/src-tauri\/src\/.*\.rs$/.test(file);
}

if (isMain(import.meta.url)) {
  const root = repoRoot();
  const failures = [];
  for (const file of trackedFiles(root).filter(inScope)) {
    for (const hit of findSensitiveLogs(readFileSync(path.join(root, file), 'utf8'))) {
      failures.push(`${file}:${hit.line}: may log ${hit.names.join(', ')}: ${hit.call}`);
    }
  }
  if (failures.length) {
    console.error('logs: FAILED (fix the call, or add a reviewed "// log-ok: <reason>" comment above it)');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log('logs: ok (no log call prints meeting content)');
}
