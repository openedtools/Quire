#!/usr/bin/env node
// PreToolUse hook for Edit, Write and MultiEdit.
//
// While LOCK_TESTS=1 (set for bug-fix tasks, after the failing test is committed) existing tests
// are locked, so a failing test cannot be weakened to make a fix pass. New test files can still
// be created. Locked: existing files under tests/ or __tests__/, *.test.* and *.spec.* files,
// *_test.rs, and the inline `#[cfg(test)] mod` section at the bottom of Rust source files.
//
// Exit 2 blocks the tool call and sends stderr back to Claude. Unset LOCK_TESTS to allow edits.
import { existsSync, readFileSync } from 'node:fs';

if (process.env.LOCK_TESTS !== '1') process.exit(0);

function block(why) {
  process.stderr.write(
    `Blocked: ${why} LOCK_TESTS=1 is set, so existing tests are locked (fix the code, not the test). ` +
      'If a test really must change, ask the owner to unset LOCK_TESTS first.\n',
  );
  process.exit(2);
}

let input;
try {
  input = JSON.parse(readFileSync(0, 'utf8'));
} catch {
  block('the hook could not read the tool input, so it fails closed.');
}

const tool = input.tool_name;
const ti = input.tool_input ?? {};
const file = ti.file_path;
if (!file || !existsSync(file)) process.exit(0); // nothing to protect, or a new file

// Lower case so that Windows folder names such as Tests or SRC match too
const norm = String(file).replace(/\\/g, '/').toLowerCase();
const TEST_DIR = /(^|\/)(tests?|__tests__)\//;
const TEST_FILE = /\.(test|spec)\.[cm]?[jt]sx?$|_test\.rs$/;
if (TEST_DIR.test(norm) || TEST_FILE.test(norm)) block(`${file} is an existing test file.`);

if (!norm.endsWith('.rs')) process.exit(0);

// Compare with line endings normalised: files on Windows are often CRLF, tool input is LF
const lf = (s) => String(s ?? '').replace(/\r\n/g, '\n');
const content = lf(readFileSync(file, 'utf8'));
const m = /#\[cfg\(test\)\]\s*(?:pub\s+)?mod\s+\w+/.exec(content);
if (!m) process.exit(0);
const start = m.index;

if (tool === 'Write') {
  if (!lf(ti.content).includes(content.slice(start))) {
    block(`${file} has an inline test module and this rewrite changes it.`);
  }
  process.exit(0);
}

const edits = tool === 'MultiEdit' ? (ti.edits ?? []) : [ti];
for (const e of edits) {
  const old = lf(e.old_string);
  if (!old) continue;
  let i = content.indexOf(old);
  while (i !== -1) {
    if (i + old.length > start) block(`this edit touches the inline test module of ${file}.`);
    if (!e.replace_all) break;
    i = content.indexOf(old, i + 1);
  }
}
process.exit(0);
