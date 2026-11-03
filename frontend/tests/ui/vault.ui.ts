import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// The Sync screen (spec F-SYNC-1, F-SYNC-2, F-SYNC-9; AC-SYNC-8), mocked Tauri core.
const MOCK = path.join(__dirname, 'mock-tauri.js');
const FOLDER = '/Users/example/Library/Mobile Documents/com~apple~CloudDocs/Quire';

type Call = [string, Record<string, unknown>];

async function open(page: Page, enabled: boolean) {
  await page.addInitScript({ path: MOCK });
  await page.addInitScript(
    ({ on, folder }) => {
      const state = {
        enabled: on,
        folder: on ? folder : null,
        last_export_at: on ? '2026-11-02T10:00:00.000Z' : null,
        last_report: on ? { written: 0, unchanged: 12, changed_in_vault: 1, moved: 0, deleted: 0, failed: 0, notes: 10, people: 1, folders: 1, verified: true } : null,
        suggested_folder: folder,
        long_path: false,
      };
      const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__;
      const base = internals.invoke.bind(internals);
      internals.invoke = async (cmd, args = {}) => {
        const result = await base(cmd, args);
        if (cmd === 'api_vault_status') return { ...state };
        if (cmd === 'api_vault_enable') {
          state.enabled = true;
          state.folder = String(args.folder);
          return { ...state };
        }
        if (cmd === 'api_vault_disable') {
          state.enabled = false;
          return { ...state };
        }
        if (cmd === 'api_vault_remove_files') {
          state.enabled = false;
          return { removed: 14, kept: 1 };
        }
        return result;
      };
    },
    { on: enabled, folder: FOLDER },
  );
  await page.goto('/settings');
  await page.waitForLoadState('networkidle');
  await page.getByRole('tab', { name: 'Sync' }).click();
  await page.waitForTimeout(400);
}

const named = async (page: Page, cmd: string) =>
  (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === cmd);

test('nothing is written until the folder is chosen and the iCloud disclosure is accepted', async ({ page }) => {
  await open(page, false);
  await page.getByRole('button', { name: 'Use iCloud Drive' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText(/Apple's iCloud software uploads the files, not this app/)).toBeVisible();
  await expect(dialog.getByText(/Without Advanced Data Protection/)).toBeVisible();
  await expect(dialog.getByText(/Audio, voiceprints, API keys and settings are never written there/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  expect(await named(page, 'api_vault_enable')).toHaveLength(0);

  await page.getByRole('button', { name: 'Use iCloud Drive' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Write the vault' }).click();
  expect((await named(page, 'api_vault_enable')).at(-1)?.[1]).toEqual({ folder: FOLDER });
  await expect(page.getByText(/Audio is never written to the vault/)).toBeVisible();
});

test('the status says what is in the vault and what was left alone; turning off keeps the files', async ({ page }) => {
  await open(page, true);
  await expect(page.getByText(/In the vault: 10 meetings, 1 person, 1 folder/)).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: '1 file was changed in the vault folder' })).toBeVisible();
  await page.getByRole('switch', { name: 'Write the vault' }).click();
  expect(await named(page, 'api_vault_disable')).toHaveLength(1);
  expect(await named(page, 'api_vault_remove_files')).toHaveLength(0);
});

test("removing Quire's files is a separate, confirmed action", async ({ page }) => {
  await open(page, true);
  await page.getByRole('button', { name: "Remove Quire's files from the vault" }).click();
  await expect(page.getByRole('dialog').getByText(/Your meetings in Quire are not touched/)).toBeVisible();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  expect(await named(page, 'api_vault_remove_files')).toHaveLength(0);
  await page.getByRole('button', { name: "Remove Quire's files from the vault" }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Remove the files' }).click();
  expect(await named(page, 'api_vault_remove_files')).toHaveLength(1);
});
