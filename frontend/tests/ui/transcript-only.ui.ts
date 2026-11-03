import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Transcript-only mode (spec F-TO-1, F-TO-3, F-TO-4, F-TO-5; AC-TO-2, AC-TO-4), mocked Tauri core.
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

test('the switch next to Record is saved as a preference and states what it does not change', async ({ page }) => {
  await open(page, '/');
  const toggle = page.getByRole('switch', { name: 'Transcript only: do not save audio' });
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect(toggle).toBeChecked();
  const saved = await named(page, 'set_recording_preferences');
  expect((saved.at(-1)?.[1].preferences as { transcript_only: boolean }).transcript_only).toBe(true);
  await expect(page.getByText("Whether you need people's consent depends on where you and they are, and it does not change with this setting.")).toBeVisible();
  await expect(page.getByText(/avoid|legal risk|compliant/i)).toHaveCount(0);
  // Remembered between sessions: the choice is saved in the recording preferences, which Rust stores
});

test('a transcript-only meeting says why speaker detection is unavailable', async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-2');
  await expect(page.getByTestId('audio-status')).toHaveText('No audio saved');
  await page.getByRole('button', { name: 'Speakers' }).click();
  await page.getByRole('button', { name: /Detect speakers/ }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('recorded as transcript only, so no audio was saved');
  expect(await named(page, 'api_detect_speakers')).toEqual([]);
});

test('the retention settings explain Off against transcript only in one sentence', async ({ page }) => {
  await open(page, '/settings');
  await page.locator('[role="tab"][data-value="recording"], [role="tab"][id$="-trigger-recording"]').first().click();
  await expect(page.getByText('Off saves the audio briefly so speakers can be detected, then deletes it.', { exact: false })).toBeVisible();
});
