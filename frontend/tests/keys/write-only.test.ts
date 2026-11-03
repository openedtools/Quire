import { describe, expect, test } from 'bun:test';
import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';

// Spec F-MOD-6, AC-MOD-2: the window can save or remove a key but never reads one back.
const ROOT = path.resolve(import.meta.dir, '..', '..', '..');
const tracked = execSync('git ls-files frontend/src frontend/src-tauri/src', { cwd: ROOT, encoding: 'utf8' }).split('\n').filter(Boolean);
const read = (file: string) => readFileSync(path.join(ROOT, file), 'utf8');

describe('API keys are write-only for the window', () => {
  test('no window code asks the core for a saved key', () => {
    const offenders = tracked.filter((f) => /\.(ts|tsx)$/.test(f)).filter((f) => /api_get_(transcript_)?api_key/.test(read(f)));
    expect(offenders).toEqual([]);
  });

  test('the core has no command that returns a key', () => {
    const lib = read('frontend/src-tauri/src/lib.rs');
    expect(lib).not.toMatch(/api_get_(transcript_)?api_key/);
    expect(lib).toMatch(/api_has_api_key/);
  });

  test('config commands send an empty key field and say only whether one is saved', () => {
    const api = read('frontend/src-tauri/src/api/api.rs');
    expect(api).toMatch(/api_key: None,\s*has_api_key: api_key\.is_some\(\)/);
    expect(api).toMatch(/CustomOpenAIConfig \{ api_key: None, \.\.c \}/);
  });
});
