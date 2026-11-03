import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
// @ts-ignore plain ES module without types
import { applyAll, applyToTauriConf, checkAll, renderRs, renderTs, validateBrand } from '../../scripts/brand-apply.mjs';

const REPO = path.resolve(import.meta.dir, '../../..');
const realBrand = JSON.parse(readFileSync(path.join(REPO, 'brand/brand.json'), 'utf8'));
const realConf = readFileSync(path.join(REPO, 'frontend/src-tauri/tauri.conf.json'), 'utf8');

const brand = (over: Record<string, unknown> = {}) => ({ ...realBrand, ...over });

describe('validateBrand', () => {
  test('accepts the repository brand file', () => {
    expect(validateBrand(realBrand)).toEqual([]);
  });

  test.each([
    ['name', ''],
    ['slug', 'Quire'],
    ['slug', 'has space'],
    ['bundleId', 'notes'],
    ['bundleId', 'com.Open.Tools'],
    ['windowTitle', 'say "hi"'],
    ['tagline', 'back\\slash'],
    ['tagline', 'with ' + String.fromCharCode(0x2014) + ' dash'],
  ])('rejects a bad %s (%p)', (key, value) => {
    const problems = validateBrand(brand({ [key]: value }));
    expect(problems.length).toBeGreaterThan(0);
    expect(problems.join(' ')).toContain(key);
  });

  test('needs no legacy block: the earlier product is no longer recognised (plan PR2)', () => {
    expect(validateBrand(brand({ legacy: undefined }))).toEqual([]);
  });
});

describe('applyToTauriConf', () => {
  const next = brand({ name: 'Marginalia', slug: 'marginalia', windowTitle: 'Marginalia', bundleId: 'com.openedtools.notes' });

  test('sets the four identity fields and leaves everything else as it was', () => {
    const out = applyToTauriConf(realConf, next);
    const before = JSON.parse(realConf);
    const after = JSON.parse(out);
    expect(after.productName).toBe('Marginalia');
    expect(after.mainBinaryName).toBe('marginalia');
    expect(after.identifier).toBe('com.openedtools.notes');
    expect(after.app.windows[0].title).toBe('Marginalia');
    // Everything else is untouched
    const strip = (c: any) => {
      const copy = JSON.parse(JSON.stringify(c));
      delete copy.productName; delete copy.mainBinaryName; delete copy.identifier; delete copy.app.windows[0].title;
      return copy;
    };
    expect(strip(after)).toEqual(strip(before));
  });

  test('is idempotent', () => {
    const once = applyToTauriConf(realConf, next);
    expect(applyToTauriConf(once, next)).toBe(once);
  });

  test('a CRLF file (a Windows checkout) is edited correctly and stays CRLF', () => {
    // Without a mainBinaryName line, so the insertion path is exercised on CRLF too
    const withoutBinaryName = realConf.split('\n').filter((l: string) => !l.includes('mainBinaryName')).join('\n');
    const crlf = withoutBinaryName.replace(/\n/g, '\r\n');
    const out = applyToTauriConf(crlf, next);
    expect(JSON.parse(out).mainBinaryName).toBe('marginalia');
    expect(out.includes('\r\n')).toBe(true);
    expect(out.replace(/\r\n/g, '').includes('\n')).toBe(false); // no bare LF left behind
    expect(applyToTauriConf(out, next)).toBe(out);
  });

  test('keeps the original formatting (a line-level diff of a few lines)', () => {
    const out = applyToTauriConf(realConf, next).split('\n');
    const orig = realConf.split('\n');
    // Count lines that are new, ignoring position (an inserted line shifts everything after it)
    const available = new Map<string, number>();
    for (const l of orig) available.set(l, (available.get(l) ?? 0) + 1);
    let added = 0;
    for (const l of out) {
      const n = available.get(l) ?? 0;
      if (n > 0) available.set(l, n - 1);
      else added += 1;
    }
    expect(Math.abs(out.length - orig.length)).toBeLessThanOrEqual(1);
    expect(added).toBeLessThanOrEqual(4); // productName, mainBinaryName, identifier, title
    expect(added).toBeGreaterThan(0);
  });
});

describe('renderers', () => {
  test('TypeScript module exposes the brand', () => {
    const ts = renderTs(realBrand);
    expect(ts).toContain('GENERATED');
    expect(ts).toContain(`name: "${realBrand.name}"`);
    expect(ts).toContain(`bundleId: "${realBrand.bundleId}"`);
  });

  test('Rust module exposes the brand constants and nothing from the earlier product', () => {
    const rs = renderRs(realBrand);
    expect(rs).toContain('GENERATED');
    expect(rs).toContain(`pub const BUNDLE_ID: &str = "${realBrand.bundleId}";`);
    expect(rs).not.toContain('LEGACY_');
    expect(rs).toContain(`pub const TEMP_PREFIX: &str = ".${realBrand.slug}_decode_";`);
  });
});

describe('apply and check against a repository tree', () => {
  let root: string;
  beforeAll(() => {
    root = mkdtempSync(path.join(tmpdir(), 'brand-'));
    mkdirSync(path.join(root, 'brand'), { recursive: true });
    mkdirSync(path.join(root, 'frontend/src-tauri/src'), { recursive: true });
    mkdirSync(path.join(root, 'frontend/src/lib'), { recursive: true });
    cpSync(path.join(REPO, 'brand/brand.json'), path.join(root, 'brand/brand.json'));
    cpSync(path.join(REPO, 'frontend/src-tauri/tauri.conf.json'), path.join(root, 'frontend/src-tauri/tauri.conf.json'));
  });
  afterAll(() => rmSync(root, { recursive: true, force: true }));

  test('check reports drift before apply, none after, and drift again after a rename', () => {
    expect(checkAll(root).length).toBeGreaterThan(0);
    applyAll(root);
    expect(existsSync(path.join(root, 'frontend/src/lib/brand.generated.ts'))).toBe(true);
    expect(existsSync(path.join(root, 'frontend/src-tauri/src/brand_generated.rs'))).toBe(true);
    expect(checkAll(root)).toEqual([]);

    // A Windows checkout may turn every line ending into CRLF: that is not drift
    for (const f of ['frontend/src/lib/brand.generated.ts', 'frontend/src-tauri/src/brand_generated.rs', 'frontend/src-tauri/tauri.conf.json']) {
      const full = path.join(root, f);
      writeFileSync(full, readFileSync(full, 'utf8').replace(/\r?\n/g, '\r\n'));
    }
    expect(checkAll(root)).toEqual([]);

    const file = path.join(root, 'brand/brand.json');
    writeFileSync(file, JSON.stringify({ ...realBrand, name: 'Marginalia' }, null, 2));
    const drift = checkAll(root);
    expect(drift.length).toBeGreaterThan(0);
    applyAll(root);
    expect(checkAll(root)).toEqual([]);
  });
});
