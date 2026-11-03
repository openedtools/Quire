import { expect, test } from '@playwright/test';
import path from 'node:path';

// A yes covers exactly the meetings the person was shown (spec F-RET-13, Phase K review).
const MOCK = path.join(__dirname, 'mock-tauri.js');
type Call = [string, Record<string, unknown>];

test('confirming many deletions sends back the meetings that were listed', async ({ page }) => {
  await page.addInitScript({ path: MOCK });
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
  await page.evaluate(() =>
    (window as unknown as { __emit: (e: string, p: unknown) => void }).__emit('retention-needs-confirmation', { count: 2, ids: ['meeting-1', 'meeting-2'] }),
  );
  await page.getByRole('button', { name: 'Delete their audio' }).click();
  const calls = (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === 'api_retention_confirm_due');
  expect(calls[0][1]).toEqual({ count: 2, ids: ['meeting-1', 'meeting-2'] });
});
