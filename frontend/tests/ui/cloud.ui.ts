import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// The cloud switch (spec F-MOD-5, AC-MOD-4, AC-QA-5), mocked Tauri core.
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

/** Adds the cloud commands to the mock: off unless `allowed`, with an optional upgrade notice. */
async function open(page: Page, url: string, cloud: { allowed: boolean; notice: string | null }) {
  await page.addInitScript({ path: MOCK });
  await page.addInitScript((initial) => {
    const state = { ...initial };
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__;
    const base = internals.invoke.bind(internals);
    internals.invoke = async (cmd, args = {}) => {
      const result = await base(cmd, args);
      if (cmd === 'api_cloud_status') return { ...state };
      if (cmd === 'api_set_cloud_allowed') {
        state.allowed = Boolean(args.allowed);
        state.notice = null;
        return { ...state };
      }
      if (cmd === 'api_cloud_dismiss_notice') state.notice = null;
      return result;
    };
  }, cloud);
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

const named = async (page: Page, cmd: string) =>
  (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === cmd);

async function privacyTab(page: Page) {
  await page.getByRole('tab', { name: 'Privacy' }).click();
  return page.getByRole('switch', { name: 'Allow sending data to cloud services' });
}

test('turning cloud services on asks once and names what is sent; Cancel changes nothing', async ({ page }) => {
  await open(page, '/settings', { allowed: false, notice: null });
  const toggle = await privacyTab(page);
  await expect(toggle).not.toBeChecked();

  await toggle.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Allow sending data to cloud services?')).toBeVisible();
  await expect(dialog.getByText(/send the meeting transcript and your notes to the provider you choose/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(toggle).not.toBeChecked();
  expect(await named(page, 'api_set_cloud_allowed')).toHaveLength(0);

  await toggle.click();
  await page.getByRole('dialog').getByRole('button', { name: 'Allow' }).click();
  await expect(toggle).toBeChecked();
  expect((await named(page, 'api_set_cloud_allowed')).at(-1)?.[1]).toEqual({ allowed: true });
});

test('turning cloud services off needs no confirmation', async ({ page }) => {
  await open(page, '/settings', { allowed: true, notice: null });
  const toggle = await privacyTab(page);
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(toggle).not.toBeChecked();
  expect((await named(page, 'api_set_cloud_allowed')).at(-1)?.[1]).toEqual({ allowed: false });
});

test('an upgrade that kept cloud services on says so once, with one click to turn them off', async ({ page }) => {
  await open(page, '/', { allowed: true, notice: 'Anthropic Claude' });
  await expect(page.getByText(/already sent summaries to Anthropic Claude/)).toBeVisible();
  await page.getByRole('button', { name: 'Turn off' }).click();
  expect((await named(page, 'api_set_cloud_allowed')).at(-1)?.[1]).toEqual({ allowed: false });
});
