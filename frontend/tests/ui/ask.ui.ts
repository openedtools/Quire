import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// The Ask box on a saved meeting (spec F-QA, AC-QA-7), against the mocked Tauri core.
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

async function open(page: Page, query = '') {
  await page.addInitScript({ path: MOCK });
  await page.goto(`/meeting-details?id=meeting-1${query}`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

async function ask(page: Page, question: string) {
  await page.getByRole('button', { name: 'Ask about this meeting' }).click();
  await page.getByLabel('Question about this meeting').fill(question);
  await page.getByRole('button', { name: 'Ask', exact: true }).click();
}

test('the Ask box starts collapsed', async ({ page }) => {
  await open(page);
  await expect(page.getByRole('button', { name: 'Ask about this meeting' })).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByLabel('Question about this meeting')).toHaveCount(0);
});

test('an answer cites a time that jumps to its line', async ({ page }) => {
  await open(page);
  await ask(page, 'Who writes the board note?');
  const box = page.getByRole('region', { name: 'Ask about this meeting' });
  await expect(box.getByText('Sam drafts the board note by Friday')).toBeVisible();
  await expect(box.getByText('AI generated. Answered from the whole transcript')).toBeVisible();
  await page.getByRole('button', { name: 'Go to 00:28' }).click();
  await expect(page.locator('#segment-t3')).toHaveClass(/bg-accent-tint/);
  await page.screenshot({ path: path.join(OUT, `08-ask-${test.info().project.name}.png`) });
});

test('a hostile answer renders inert', async ({ page }) => {
  await open(page);
  await ask(page, 'hostile');
  const box = page.getByRole('region', { name: 'Ask about this meeting' });
  await expect(box.getByText('Bold')).toBeVisible();
  await expect(box.locator('img')).toHaveCount(0);
  await expect(box.locator('script')).toHaveCount(0);
  await expect(box.locator('a[href^="http"]')).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)).toBeUndefined();
});

test('Add to note appends a labelled answer to My notes', async ({ page }) => {
  await open(page);
  await ask(page, 'Who writes the board note?');
  await page.getByRole('button', { name: 'Add to note' }).click();
  await page.getByRole('tab', { name: 'My notes' }).click();
  await expect(page.getByText('Ask (AI generated):')).toBeVisible();
});

test('with no model the box asks for a download', async ({ page }) => {
  await open(page, '&noaskmodel');
  await page.getByRole('button', { name: 'Ask about this meeting' }).click();
  await expect(page.getByText('Download a model in Settings.')).toBeVisible();
  await expect(page.getByLabel('Question about this meeting')).toBeDisabled();
});

test('a provider off this computer is named', async ({ page }) => {
  await open(page, '&askcloud');
  await page.getByRole('button', { name: 'Ask about this meeting' }).click();
  await expect(page.getByText('Sends the transcript to OpenAI.')).toBeVisible();
});

test('a jump link the model wrote itself does nothing', async ({ page }) => {
  await open(page);
  await ask(page, 'hostile');
  const box = page.getByRole('region', { name: 'Ask about this meeting' });
  await expect(box.getByText('far')).toBeVisible();
  await expect(box.getByRole('button', { name: 'Go to far' })).toHaveCount(0);
  await expect(box.getByRole('button', { name: 'Go to 00:09' })).toHaveCount(1);
});
