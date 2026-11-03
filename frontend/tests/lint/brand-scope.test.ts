import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { scanned } from '../../scripts/lint/brand.mjs';

describe('scanned', () => {
  test('covers every tracked file, documentation and scripts included', () => {
    expect(scanned('README.md')).toBe(true);
    expect(scanned('PRIVACY_POLICY.md')).toBe(true);
    expect(scanned('CLAUDE.md')).toBe(true);
    expect(scanned('intent/private-meeting-recorder/plan.md')).toBe(true);
    expect(scanned('frontend/src/components/About.tsx')).toBe(true);
    expect(scanned('frontend/scripts/release.sh')).toBe(true);
    expect(scanned('.github/force-portable-ggml.cmake')).toBe(true);
  });
  test('skips only the licence, the third-party notices and applied migrations', () => {
    expect(scanned('LICENSE.md')).toBe(false);
    expect(scanned('THIRD_PARTY_NOTICES.md')).toBe(false);
    expect(scanned('frontend/src-tauri/migrations/20250916100000_initial_schema.sql')).toBe(false);
    expect(scanned('frontend/LICENSE.md')).toBe(true);
  });
});
