import { describe, expect, test } from 'bun:test';
import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';

// Plan PK2: the line endings each platform's database was created with must not drift.
const ROOT = path.resolve(import.meta.dir, '..', '..', '..');

describe('migration line endings', () => {
  test('the repository stores every migration with LF, as Mac builds embed them', () => {
    // What git stores (`i/`), not the checkout, which is CRLF on Windows by design
    const rows = execSync('git ls-files --eol frontend/src-tauri/migrations', { cwd: ROOT, encoding: 'utf8' }).trim().split('\n');
    expect(rows.length).toBeGreaterThan(10);
    for (const row of rows) expect(row.startsWith('i/lf')).toBe(true);
  });

  test('git checks migrations out with each system\'s own line endings', () => {
    expect(readFileSync(path.join(ROOT, '.gitattributes'), 'utf8')).toMatch(/^frontend\/src-tauri\/migrations\/\*\.sql text$/m);
  });

  test('the Windows build converts every migration to CRLF, as Windows databases expect', () => {
    const script = readFileSync(path.join(ROOT, 'frontend/scripts/build-windows-on-mac.sh'), 'utf8');
    expect(script).toContain('glob("*.sql")');
    expect(script).toContain('.replace(b"\\r\\n", b"\\n")');
    expect(script).toContain('data.replace(b"\\n", b"\\r\\n")');
    expect(script).toContain('frontend/src-tauri/migrations');
  });
});
