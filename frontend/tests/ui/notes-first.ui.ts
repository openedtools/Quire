import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Notes-first meetings (spec F-NM, AC-NM-1 to AC-NM-3), against the mocked Tauri core.
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

async function open(page: Page, url = '/') {
  await page.addInitScript({ path: MOCK });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
  const toggle = page.getByRole('button', { name: 'Show meeting list' });
  if (await toggle.count()) await toggle.click();
}

const calls = (page: Page) => page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs);
const named = async (page: Page, cmd: string) => (await calls(page)).filter(([c]) => c === cmd);

async function newMeeting(page: Page) {
  await page.getByRole('button', { name: 'New meeting' }).first().click();
  await expect(page).toHaveURL(/meeting-details\?id=meeting-new-/);
  await expect(page.getByText('No recording yet')).toBeVisible();
}

test('New meeting opens a meeting ready for notes, without recording', async ({ page }) => {
  await open(page);
  const started = Date.now();
  await newMeeting(page);
  const editor = page.locator('[contenteditable="true"]').first();
  await expect(editor).toBeFocused();
  console.log(`New meeting to focused notes: ${Date.now() - started} ms (dev server)`);
  await page.keyboard.type('Agenda: venue, budget');
  await expect(editor).toContainText('Agenda: venue, budget');
  const recordingCalls = (await calls(page)).filter(([c]) => /recording/.test(c) && c !== 'get_recording_preferences' && c !== 'get_recording_state' && c !== 'is_recording');
  expect(recordingCalls.filter(([c]) => c.startsWith('start_'))).toEqual([]);
  const [[, args]] = await named(page, 'api_create_meeting');
  expect(String(args.title)).toMatch(/^Meeting, \d{1,2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}$/);
  await page.screenshot({ path: path.join(OUT, `09-new-meeting-${test.info().project.name}.png`) });
});

test('leaving an untouched new meeting removes it', async ({ page }) => {
  await open(page);
  await newMeeting(page);
  const title = String((await named(page, 'api_create_meeting'))[0][1].title);
  await expect(page.getByText(title).first()).toBeVisible();
  await page.getByRole('button', { name: 'Home' }).first().click();
  await expect.poll(async () => (await named(page, 'api_discard_empty_meeting')).length, { timeout: 5000 }).toBe(1);
  const [[, args]] = await named(page, 'api_discard_empty_meeting');
  expect(args.defaultTitle).toBe(title);
  await expect(page.getByText(title)).toHaveCount(0);
});

test('a new meeting with notes stays after leaving', async ({ page }) => {
  await open(page);
  await newMeeting(page);
  const title = String((await named(page, 'api_create_meeting'))[0][1].title);
  await page.keyboard.type('Keep this');
  await page.waitForTimeout(1500);
  await page.getByRole('button', { name: 'Home' }).first().click();
  await expect.poll(async () => (await named(page, 'api_discard_empty_meeting')).length, { timeout: 5000 }).toBe(1);
  await page.waitForTimeout(300);
  await expect(page.getByText(title).first()).toBeVisible();
});

test('Record in a new meeting records under its title and keeps the meeting', async ({ page }) => {
  await open(page);
  await newMeeting(page);
  const title = String((await named(page, 'api_create_meeting'))[0][1].title);
  const id = new URL(page.url()).searchParams.get('id');
  await page.getByRole('button', { name: 'Record', exact: true }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect.poll(async () => (await named(page, 'start_recording_with_devices_and_meeting')).length, { timeout: 10000 }).toBe(1);
  const [[, args]] = await named(page, 'start_recording_with_devices_and_meeting');
  expect(args.meetingName).toBe(title);
  const target = await page.evaluate(() => sessionStorage.getItem('quire_record_into_meeting'));
  expect(JSON.parse(target ?? '{}')).toEqual({ id, title });
  await page.waitForTimeout(2500);
  expect(await named(page, 'api_discard_empty_meeting')).toEqual([]);
});

test('the Record shortcut never records into a notes-first meeting', async ({ page }) => {
  // A target left behind (for example by a start that failed) is dropped by Record now
  await open(page);
  await page.evaluate(() => sessionStorage.setItem('quire_record_into_meeting', JSON.stringify({ id: 'meeting-stale', title: 'Stale' })));
  await page.getByRole('button', { name: /Record now|Start recording/ }).first().click();
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem('quire_record_into_meeting')), { timeout: 5000 }).toBeNull();
});
