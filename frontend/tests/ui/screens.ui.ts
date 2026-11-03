import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

async function open(page: Page, url: string) {
  await page.addInitScript({ path: MOCK });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

async function showSidebar(page: Page) {
  const toggle = page.getByRole('button', { name: 'Show meeting list' });
  if (await toggle.count()) {
    await toggle.click();
    await page.waitForTimeout(400);
  }
}

async function shot(page: Page, name: string) {
  const theme = test.info().project.name;
  await page.screenshot({ path: path.join(OUT, `${name}-${theme}.png`) });
}

test('record screen', async ({ page }) => {
  await open(page, '/');
  await shot(page, '01-record');
  await showSidebar(page);
  await shot(page, '01-record-sidebar');
});

test('meeting: transcript and summary', async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-1');
  await shot(page, '02-meeting');
});

test('meeting: my notes', async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-1');
  await page.getByRole('tab', { name: 'My notes' }).click();
  await page.waitForTimeout(800);
  await shot(page, '02b-meeting-notes');
});

test('people', async ({ page }) => {
  await open(page, '/people');
  await shot(page, '03-people');
  const first = page.getByText('Alex Morgan').first();
  if (await first.isVisible()) {
    await first.click();
    await page.waitForTimeout(500);
    await shot(page, '04-person');
  }
});

for (const tab of ['general', 'recording', 'Transcriptionmodels', 'summaryModels', 'beta']) {
  test(`settings: ${tab}`, async ({ page }) => {
    await open(page, '/settings');
    const trigger = page.locator(`[role="tab"][data-value="${tab}"], [role="tab"][id$="-trigger-${tab}"]`).first();
    if (await trigger.count()) await trigger.click();
    await page.waitForTimeout(500);
    await shot(page, `05-settings-${tab}`);
  });
}

test('onboarding', async ({ page }) => {
  await open(page, '/?onboarding');
  await shot(page, '06-onboarding-1');
  const next = page.getByRole('button', { name: /get started|continue|next/i }).first();
  if (await next.isVisible()) {
    await next.click();
    await page.waitForTimeout(500);
    await shot(page, '06-onboarding-2');
  }
});

test('dialog: delete meeting', async ({ page }) => {
  await open(page, '/');
  await showSidebar(page);
  await page.getByText('Course design sync').first().hover();
  // The delete button only shows on hover, so click it directly
  await page.getByRole('button', { name: 'Delete meeting' }).first().click({ force: true });
  await page.waitForTimeout(400);
  await shot(page, '07-dialog-delete');
});

test('about', async ({ page }) => {
  await open(page, '/');
  await page.getByRole('button', { name: 'About Quire' }).first().click();
  await page.waitForTimeout(500);
  await page.locator('summary', { hasText: 'Third-party notices' }).click();
  await page.waitForTimeout(300);
  await shot(page, '08-about');
});

test('no page errors on the main screens', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  for (const url of ['/', '/meeting-details?id=meeting-1', '/people', '/settings']) await open(page, url);
  expect(errors).toEqual([]);
});
