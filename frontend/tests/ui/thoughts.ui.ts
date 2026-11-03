import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Thought mode (plan PN4) on the desktop, against mock-tauri.js's made-up data plus stand-ins
// for the Phase N commands.
const OUT = path.join(__dirname, 'screenshots');

async function open(page: Page, url: string, kind: string | null = null) {
  await page.addInitScript({ path: path.join(__dirname, 'mock-tauri.js') });
  await page.addInitScript((k) => {
    const w = window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> } };
    const base = w.__TAURI_INTERNALS__.invoke.bind(w.__TAURI_INTERNALS__);
    w.__TAURI_INTERNALS__.invoke = (c, a) => {
      if (c === 'api_meeting_kind') return Promise.resolve(k);
      if (c === 'api_mark_thoughts') return Promise.resolve(null);
      return base(c, a);
    };
  }, kind);
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

test('Record my thoughts starts a Thoughts recording with no consent reminder', async ({ page }) => {
  await open(page, '/');
  await page.getByRole('button', { name: 'Record my thoughts' }).click();
  await page.waitForTimeout(1500);
  const args = await page.evaluate(() => (window as unknown as { __mockArgs: [string, Record<string, unknown>][] }).__mockArgs);
  const start = args.find(([c]) => c === 'start_recording_with_devices_and_meeting');
  expect(start, 'the recording started').toBeTruthy();
  expect(JSON.stringify(start?.[1])).toMatch(/Thoughts, \d{1,2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}/);
  await expect(page.getByRole('dialog', { name: "Tell people you're recording" })).toHaveCount(0);
});

test('a recording of thoughts offers Make it a prompt', async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-1', 'thoughts');
  await page.getByRole('tab', { name: 'Summary' }).click();
  await expect(page.getByRole('button', { name: /Make (it a|the) prompt/ })).toBeVisible();
  await page.screenshot({ path: path.join(OUT, `26-thoughts-prompt-${test.info().project.name}.png`) });
});
