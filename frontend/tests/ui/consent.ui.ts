import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Consent reminder, guidance and first-run notice (spec F-CON-1 to F-CON-6; AC-CON-1 to AC-CON-3).
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

async function open(page: Page, url: string) {
  await page.addInitScript({ path: MOCK });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

const calls = (page: Page) => page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs);
const named = async (page: Page, cmd: string) => (await calls(page)).filter(([c]) => c === cmd);
const stored = (page: Page, key: string) =>
  page.evaluate((k) => (window as unknown as { __storeValue: (p: string, k: string) => unknown }).__storeValue('preferences.json', k), key);

async function startRecording(page: Page) {
  const toggle = page.getByRole('button', { name: 'Show meeting list' });
  if (await toggle.count()) await toggle.click();
  await page.getByRole('button', { name: 'Record now' }).first().click();
}

test('the reminder appears after recording starts and never says the user is compliant', async ({ page }) => {
  await open(page, '/');
  await startRecording(page);
  const card = page.getByRole('dialog', { name: "Tell people you're recording" });
  await expect(card).toBeVisible();
  const order = (await calls(page)).map(([c]) => c);
  expect(order.indexOf('start_recording_with_devices_and_meeting')).toBeGreaterThanOrEqual(0);
  await expect(card).toContainText('not legal advice');
  await expect(page.getByText(/US compliance required|you are compliant|you are not compliant/i)).toHaveCount(0);
  await page.screenshot({ path: path.join(OUT, `12-consent-${test.info().project.name}.png`), animations: 'disabled' });

  // Closing without choosing only dismisses it
  await card.getByRole('button', { name: 'Close' }).click();
  await expect(card).toHaveCount(0);
  expect(await stored(page, 'show_recording_notification')).not.toBe(false);
});

test('never show again is stored and can be turned back on under Privacy', async ({ page }) => {
  await open(page, '/');
  await startRecording(page);
  const card = page.getByRole('dialog', { name: "Tell people you're recording" });
  await card.getByLabel('Never show this again').check();
  await card.getByRole('button', { name: 'Got it' }).click();
  expect(await stored(page, 'show_recording_notification')).toBe(false);

  // Inside the app (a reload would reset the mocked store; the real one is on disk)
  await page.getByRole('button', { name: /^Settings$/ }).first().click();
  await page.waitForTimeout(600);
  await page.locator('[role="tab"][data-value="privacy"], [role="tab"][id$="-trigger-privacy"]').first().click();
  const reminder = page.getByRole('switch', { name: 'Recording reminder' });
  await expect(reminder).not.toBeChecked();
  await reminder.click();
  expect(await stored(page, 'show_recording_notification')).toBe(true);
  await expect(page.getByText('Do not record anything you are not allowed to record')).toBeVisible();
});

test('read more opens the guidance, and the outside guide goes through the guidance allowlist', async ({ page }) => {
  await open(page, '/');
  await startRecording(page);
  await page.getByRole('button', { name: 'Read more' }).click();
  await expect(page.getByRole('heading', { name: "Telling people you're recording" })).toBeVisible();
  await expect(page.getByText("I'm using a tool to transcribe this meeting so I can take notes.")).toBeVisible();
  await page.getByRole('button', { name: "Open the Reporter's Recording Guide" }).click();
  expect(await named(page, 'open_guidance_url')).toEqual([['open_guidance_url', { url: 'https://www.rcfp.org/reporters-recording-guide/' }]]);
  expect(await named(page, 'open_external_url')).toEqual([]);
});

test('the first-run notice shows once', async ({ page }) => {
  await open(page, '/?firstrun');
  const notice = page.getByRole('dialog', { name: /Before you use/ });
  await expect(notice).toContainText('For personal use.');
  await notice.getByRole('button', { name: 'Continue' }).click();
  await expect(notice).toHaveCount(0);
  expect(await stored(page, 'first_run_notice_acknowledged')).toBe(true);
});
