import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import path from 'node:path';

// The Models page (spec F-MOD-1, F-MOD-2, AC-MOD-1), mocked Tauri core with the real models.json.
const MOCK = path.join(__dirname, 'mock-tauri.js');
const MANIFEST = JSON.parse(readFileSync(path.join(__dirname, '..', '..', 'src-tauri', 'models.json'), 'utf8'));

async function open(page: Page, language: string) {
  await page.addInitScript({ path: MOCK });
  await page.addInitScript(
    ({ manifest, lang }) => {
      localStorage.setItem('primaryLanguage', lang);
      const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
      const base = internals.invoke.bind(internals);
      internals.invoke = async (cmd, args) => {
        const result = await base(cmd, args);
        return cmd === 'api_model_manifest' ? manifest : result;
      };
    },
    { manifest: MANIFEST, lang: language },
  );
  await page.goto('/settings');
  await page.waitForLoadState('networkidle');
  await page.getByRole('tab', { name: 'Models' }).click();
  await page.waitForTimeout(500);
}

test('one Models page: transcription with Parakeet first, then summaries and Ask, with sizes, languages and sources', async ({ page }) => {
  await open(page, 'auto');
  await expect(page.getByRole('heading', { name: 'Transcription' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Summaries and Ask' })).toBeVisible();
  await page.getByText('Where the transcription models come from').click();
  const rows = page.locator('table').first().locator('tbody tr');
  await expect(rows.first()).toContainText('parakeet-tdt-0.6b-v3-int8');
  await expect(rows.first()).toContainText('671 MB');
  await expect(rows.first()).toContainText('25 European languages');
  await expect(page.getByRole('button', { name: 'Open the source of parakeet-tdt-0.6b-v3-int8' })).toBeVisible();
  await expect(page.getByText(/Choose a Whisper model/)).toHaveCount(0);
});

test('a meeting language outside Parakeet list gives the Whisper recommendation', async ({ page }) => {
  await open(page, 'ja');
  await expect(page.getByRole('status').filter({ hasText: 'Choose a Whisper model' })).toBeVisible();
});
