import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Markdown export card (spec F-MD-1, F-MD-5, F-MD-6; AC-MD-1), against the mocked Tauri core.
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

async function openSettings(page: Page) {
  await page.addInitScript({ path: MOCK });
  await page.goto('/settings');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

const named = async (page: Page, cmd: string) =>
  (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === cmd);

const card = (page: Page) => page.locator('section[aria-labelledby="markdown-export-title"]');

test('nothing is exported until a folder is chosen and confirmed', async ({ page }) => {
  await openSettings(page);
  const section = card(page);
  await expect(section.getByRole('heading', { name: 'Markdown export' })).toBeVisible();
  await expect(section.getByRole('switch', { name: 'Markdown export' })).not.toBeChecked();

  await section.getByRole('button', { name: 'Choose folder' }).click();
  const confirm = section.getByRole('group', { name: 'Confirm export folder' });
  await expect(confirm).toContainText('iCloud~md~obsidian');
  await expect(confirm).toContainText('This folder is in iCloud Drive.');
  await expect(confirm).toContainText('leave this computer through that service, not through this app');
  expect(await named(page, 'api_markdown_export_enable')).toEqual([]);

  await confirm.getByRole('button', { name: 'Cancel' }).click();
  await expect(confirm).toHaveCount(0);
  expect(await named(page, 'api_markdown_export_enable')).toEqual([]);

  await section.getByRole('button', { name: 'Choose folder' }).click();
  await section.getByRole('button', { name: 'Export here' }).click();
  const enabled = await named(page, 'api_markdown_export_enable');
  expect(enabled).toHaveLength(1);
  expect(String(enabled[0][1].folder)).toContain('Documents/Vault');
  await expect(section.getByRole('switch', { name: 'Markdown export' })).toBeChecked();
  await expect(section).toContainText('3 meetings exported');
});

test('files changed elsewhere are listed and replaced only after a second click', async ({ page }) => {
  await openSettings(page);
  const section = card(page);
  await section.getByRole('button', { name: 'Choose folder' }).click();
  await section.getByRole('button', { name: 'Export here' }).click();
  await expect(section).toContainText('Budget review with finance');
  await expect(section).toContainText('Edited in another app');
  await expect(section.getByRole('switch', { name: 'Markdown export' })).toBeChecked();
  await page.screenshot({ path: path.join(OUT, `10-markdown-export-${test.info().project.name}.png`), fullPage: true, animations: 'disabled' });

  await section.getByRole('button', { name: /Replace with .+'s version/ }).click();
  expect(await named(page, 'api_markdown_export_replace')).toEqual([]);
  await section.getByRole('button', { name: 'Keep theirs' }).click();
  expect(await named(page, 'api_markdown_export_replace')).toEqual([]);

  await section.getByRole('button', { name: /Replace with .+'s version/ }).click();
  await section.getByRole('button', { name: 'Replace', exact: true }).click();
  const replaced = await named(page, 'api_markdown_export_replace');
  expect(replaced).toEqual([['api_markdown_export_replace', { meetingId: 'meeting-2' }]]);
  await expect(section).not.toContainText('Edited in another app');

  await section.getByRole('button', { name: 'Export all now' }).click();
  expect(await named(page, 'api_markdown_export_all')).toHaveLength(1);

  await section.getByRole('switch', { name: 'Markdown export' }).click();
  expect(await named(page, 'api_markdown_export_disable')).toHaveLength(1);
  await expect(section.getByRole('switch', { name: 'Markdown export' })).not.toBeChecked();
});
