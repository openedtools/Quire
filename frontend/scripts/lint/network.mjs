// Fails when a Rust call site that can reach another computer has no reviewed tag naming what
// enables it (spec NF-1, AC-HK-3). A call site is an HTTP request (`.send()` on a request
// builder, `reqwest::get`, `ureq`, a raw TCP connection), an external opener or shell (`open`,
// `explorer`, `xdg-open`, `cmd`, `rundll32`, `osascript`, `open::that`) or a library
// download. The tag is a `// net: <what enables it>` comment on the call's line or up to
// six lines above it. `pnpm lint` prints the inventory with the tags.
// Usage: node scripts/lint/network.mjs [--list]
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isMain, repoRoot, trackedFiles } from './lib.mjs';

const CALL = [
  /\.send\(\)/,
  /\breqwest::(?:blocking::)?get\(/,
  /\bCommand::new\("(?:open|explorer|xdg-open|cmd|rundll32|osascript)"\)/,
  /\bureq::|\bTcpStream::connect\b|\bopen::that\b/,
  /\b(?:check_latest_version|download_ffmpeg_package)\(/,
];
const TAG = /\/\/\s*net:\s*(\S.*)$/;
const REACH = 6;

/** The source before the first test module, which never runs in the app. */
function appSource(text) {
  const test = text.search(/#\[cfg\(test\)\]\s*(pub(\(crate\))? )?mod /);
  return test < 0 ? text : text.slice(0, test);
}

/** Returns [{ line, call, tag }] for each call site; `tag` is null when it has none. */
export function findCallSites(text) {
  const lines = appSource(text).split('\n');
  const sites = [];
  lines.forEach((line, i) => {
    if (line.trim().startsWith('//')) return;
    if (!CALL.some((re) => re.test(line))) return;
    let tag = null;
    for (let j = i; j >= Math.max(0, i - REACH); j -= 1) {
      const m = lines[j].match(TAG);
      if (m) {
        tag = m[1].trim();
        break;
      }
    }
    sites.push({ line: i + 1, call: line.trim().slice(0, 100), tag });
  });
  return sites;
}

export function inScope(file) {
  return /^(frontend\/src-tauri\/(src|build)\/.*|frontend\/src-tauri\/build|llama-helper\/src\/.*)\.rs$/.test(file);
}

if (isMain(import.meta.url)) {
  const root = repoRoot();
  const failures = [];
  const inventory = [];
  for (const file of trackedFiles(root).filter(inScope)) {
    for (const site of findCallSites(readFileSync(path.join(root, file), 'utf8'))) {
      if (site.tag) inventory.push(`${file}:${site.line}: ${site.tag}`);
      else failures.push(`${file}:${site.line}: ${site.call}`);
    }
  }
  if (process.argv.includes('--list')) for (const entry of inventory) console.log(entry);
  if (failures.length) {
    console.error('network: FAILED (add a reviewed "// net: <what enables it>" comment to each call site)');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`network: ok (${inventory.length} tagged call site(s); --list prints them)`);
}
