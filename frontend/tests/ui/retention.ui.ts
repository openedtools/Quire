import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Audio retention (spec F-RET-1, F-RET-8, F-RET-9, F-RET-13, F-RET-14; AC-RET-13), mocked Tauri core.
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

async function open(page: Page, url: string) {
  await page.addInitScript({ path: MOCK });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

const named = async (page: Page, cmd: string) =>
  (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === cmd);

test('the retention setting, its limits, detect-on-save and apply to existing', async ({ page }) => {
  await open(page, '/settings');
  await page.locator('[role="tab"][data-value="recording"], [role="tab"][id$="-trigger-recording"]').first().click();
  const section = page.locator('section[aria-labelledby="retention-title"]');
  await expect(section.getByRole('radio')).toHaveCount(6);
  await expect(section.getByRole('radio', { name: /7 days/ })).toHaveAttribute('aria-checked', 'true');
  await expect(section).toContainText('backups and disk recovery tools');
  await expect(section).toContainText('Deletion happens while the app is running');
  await expect(section).toContainText('Recording folders no meeting uses');
  await page.screenshot({ path: path.join(OUT, `11-retention-${test.info().project.name}.png`), fullPage: true, animations: 'disabled' });

  await section.getByRole('radio', { name: /^Off/ }).click();
  expect((await named(page, 'api_retention_set_settings')).at(-1)?.[1]).toEqual({ settings: { default_retention: 'off', detect_on_save: false } });
  await expect(section).toContainText('With Off, speakers cannot be detected afterwards.');

  await section.getByRole('button', { name: 'Apply to existing recordings' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('3 recordings with 52.0 MB of audio');
  await expect(dialog).toContainText('1 of them are past the new deadline');
  expect((await named(page, 'api_retention_apply_to_existing')).map(([, a]) => a.apply)).toEqual([false]);
  await dialog.getByRole('button', { name: 'Apply' }).click();
  expect((await named(page, 'api_retention_apply_to_existing')).map(([, a]) => a.apply)).toEqual([false, true]);

  await section.getByRole('switch', { name: 'Detect speakers when a recording ends' }).click();
  expect((await named(page, 'api_retention_set_settings')).at(-1)?.[1]).toEqual({ settings: { default_retention: 'off', detect_on_save: true } });
});

test("a meeting shows its audio status, and can keep it or delete it now", async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-1');
  const status = page.getByTestId('audio-status');
  await expect(status).toHaveText(/^Audio kept until /);

  await page.getByRole('button', { name: 'Audio options' }).click();
  await page.getByRole('menuitem', { name: 'Keep this one' }).click();
  await expect(status).toHaveText('Audio kept');

  await page.getByRole('button', { name: 'Audio options' }).click();
  await page.getByRole('menuitem', { name: 'Delete audio now' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('The transcript, notes and summary stay');
  expect(await named(page, 'api_meeting_delete_audio_now')).toEqual([]);
  await dialog.getByRole('button', { name: 'Delete audio' }).click();
  expect(await named(page, 'api_meeting_delete_audio_now')).toHaveLength(1);
  await expect(status).toHaveText(/^Audio deleted on /);
  await expect(page.getByRole('button', { name: 'Audio options' })).toHaveCount(0);
});

test('many recordings due at once wait for a yes', async ({ page }) => {
  await open(page, '/');
  await page.evaluate(() => (window as unknown as { __emit: (e: string, p: unknown) => void }).__emit('retention-needs-confirmation', 21));
  await expect(page.getByText('21 recordings are due for audio deletion at once')).toBeVisible();
  expect(await named(page, 'api_retention_confirm_due')).toEqual([]);
  await page.getByRole('button', { name: 'Delete their audio' }).click();
  expect((await named(page, 'api_retention_confirm_due'))[0][1]).toEqual({ count: 21 });
});
