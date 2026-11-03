import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const HOOK = path.resolve(import.meta.dir, '../../../.claude/hooks/protect-tests.mjs');

let dir: string;
const rust = () => path.join(dir, 'engine.rs');
const RUST_SOURCE = [
  'pub fn add(a: i32, b: i32) -> i32 {',
  '    a + b',
  '}',
  '',
  '#[cfg(test)]',
  'mod tests {',
  '    use super::*;',
  '    #[test]',
  '    fn adds() {',
  '        assert_eq!(add(1, 2), 3);',
  '    }',
  '}',
  '',
].join('\n');

function run(payload: unknown, lock: boolean, raw?: string) {
  const env: NodeJS.ProcessEnv = { ...process.env };
  delete env.LOCK_TESTS;
  if (lock) env.LOCK_TESTS = '1';
  const r = spawnSync(process.execPath, [HOOK], { input: raw ?? JSON.stringify(payload), env, encoding: 'utf8' });
  return { code: r.status, stderr: r.stderr };
}
const edit = (file: string, oldS: string, newS = 'x') => ({ tool_name: 'Edit', tool_input: { file_path: file, old_string: oldS, new_string: newS } });

beforeAll(() => {
  dir = mkdtempSync(path.join(tmpdir(), 'protect-tests-'));
  writeFileSync(rust(), RUST_SOURCE);
  mkdirSync(path.join(dir, 'tests'));
  writeFileSync(path.join(dir, 'tests', 'a.test.ts'), 'test("x", () => {});\n');
  writeFileSync(path.join(dir, 'plain.ts'), 'export const a = 1;\n');
  // Windows checkouts: CRLF line endings and capitalised folder names.
  // 'Tests' gets its own parent: on case-insensitive file systems it would collide with 'tests'.
  writeFileSync(path.join(dir, 'crlf.rs'), RUST_SOURCE.replace(/\n/g, '\r\n'));
  mkdirSync(path.join(dir, 'win', 'Tests'), { recursive: true });
  writeFileSync(path.join(dir, 'win', 'Tests', 'b.ts'), 'test("y", () => {});\n');
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('without LOCK_TESTS everything is allowed', () => {
  test('editing a test file passes', () => {
    expect(run(edit(path.join(dir, 'tests', 'a.test.ts'), 'test'), false).code).toBe(0);
  });
});

describe('with LOCK_TESTS=1', () => {
  test('editing an existing test file is blocked with an explanation', () => {
    const r = run(edit(path.join(dir, 'tests', 'a.test.ts'), 'test'), true);
    expect(r.code).toBe(2);
    expect(r.stderr).toContain('LOCK_TESTS');
  });

  test('editing a non-test file is allowed', () => {
    expect(run(edit(path.join(dir, 'plain.ts'), 'a = 1', 'a = 2'), true).code).toBe(0);
  });

  test('creating a brand new test file is allowed (adding tests is not weakening them)', () => {
    const payload = { tool_name: 'Write', tool_input: { file_path: path.join(dir, 'tests', 'new.test.ts'), content: 'x' } };
    expect(run(payload, true).code).toBe(0);
  });

  test('an edit inside an inline Rust test module is blocked', () => {
    expect(run(edit(rust(), 'assert_eq!(add(1, 2), 3);', 'assert_eq!(add(1, 2), 4);'), true).code).toBe(2);
  });

  test('an edit to the production part of the same Rust file is allowed', () => {
    expect(run(edit(rust(), '    a + b', '    b + a'), true).code).toBe(0);
  });

  test('a MultiEdit with one edit inside the test module is blocked', () => {
    const payload = {
      tool_name: 'MultiEdit',
      tool_input: {
        file_path: rust(),
        edits: [
          { old_string: '    a + b', new_string: '    b + a' },
          { old_string: 'fn adds()', new_string: 'fn adds_renamed()' },
        ],
      },
    };
    expect(run(payload, true).code).toBe(2);
  });

  test('rewriting a Rust file keeps its tests only if the test module is unchanged', () => {
    const keep = RUST_SOURCE.replace('    a + b', '    b + a');
    const weaken = RUST_SOURCE.replace('assert_eq!(add(1, 2), 3);', 'assert_eq!(1, 1);');
    expect(run({ tool_name: 'Write', tool_input: { file_path: rust(), content: keep } }, true).code).toBe(0);
    expect(run({ tool_name: 'Write', tool_input: { file_path: rust(), content: weaken } }, true).code).toBe(2);
  });

  test('a folder named Tests (capitalised, as on Windows) is protected too', () => {
    expect(run(edit(path.join(dir, 'win', 'Tests', 'b.ts'), 'test'), true).code).toBe(2);
  });

  test('CRLF files: a rewrite that keeps the tests is allowed, one that weakens them is blocked', () => {
    const file = path.join(dir, 'crlf.rs');
    const keep = RUST_SOURCE.replace('    a + b', '    b + a'); // LF, as the tool sends it
    const weaken = RUST_SOURCE.replace('assert_eq!(add(1, 2), 3);', 'assert_eq!(1, 1);');
    expect(run({ tool_name: 'Write', tool_input: { file_path: file, content: keep } }, true).code).toBe(0);
    expect(run({ tool_name: 'Write', tool_input: { file_path: file, content: weaken } }, true).code).toBe(2);
  });

  test('CRLF files: a multi-line edit inside the test module is blocked', () => {
    const multi = '    #[test]\n    fn adds() {\n        assert_eq!(add(1, 2), 3);';
    expect(run(edit(path.join(dir, 'crlf.rs'), multi, 'x'), true).code).toBe(2);
  });

  test('input the hook cannot read fails closed', () => {
    expect(run(null, true, 'not json').code).toBe(2);
  });
});
