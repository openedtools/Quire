import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Review findings for notes-first meetings (D5): the saved recording goes to the meeting it was
// started from, whatever the window holds by then, and a meeting you come back to is kept.
const MOCK = path.join(__dirname, 'mock-tauri.js');

type Call = [string, Record<string, unknown>];

async function open(page: Page) {
  await page.addInitScript({ path: MOCK });
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
  const toggle = page.getByRole('button', { name: 'Show meeting list' });
  if (await toggle.count()) await toggle.click();
}

const named = async (page: Page, cmd: string) =>
  (await page.evaluate(() => (window as unknown as { __mockArgs: Call[] }).__mockArgs)).filter(([c]) => c === cmd);

async function newMeeting(page: Page) {
  await page.getByRole('button', { name: 'New meeting' }).first().click();
  await expect(page.getByText('No recording yet')).toBeVisible();
  return new URL(page.url()).searchParams.get('id');
}

async function stopAndSave(page: Page) {
  // Wait for the recovery record that recording-started writes
  await page.waitForTimeout(1000);
  await page.evaluate(() => (window as unknown as { handleRecordingStop: (b: boolean) => void }).handleRecordingStop(true));
  await expect.poll(async () => (await named(page, 'api_save_transcript')).length, { timeout: 20000 }).toBe(1);
  return (await named(page, 'api_save_transcript'))[0][1];
}

test('a recording started from a meeting is saved into it, even if the window target changes', async ({ page }) => {
  await open(page);
  const id = await newMeeting(page);
  await page.getByRole('button', { name: 'Record', exact: true }).click();
  await expect.poll(async () => (await named(page, 'start_recording_with_devices_and_meeting')).length, { timeout: 10000 }).toBe(1);
  // The recording session exists, so the target is now fixed for this recording
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem('indexeddb_current_meeting_id')), { timeout: 10000 }).not.toBeNull();
  await page.evaluate(() => sessionStorage.setItem('quire_record_into_meeting', JSON.stringify({ id: 'meeting-other', title: 'Other' })));
  const args = await stopAndSave(page);
  expect(args.intoMeetingId).toBe(id);
});

test('a plain recording never lands in a meeting the window points at later', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Record now' }).click();
  await expect.poll(async () => (await named(page, 'start_recording_with_devices_and_meeting')).length, { timeout: 10000 }).toBe(1);
  // The recording session exists, so the target is now fixed for this recording
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem('indexeddb_current_meeting_id')), { timeout: 10000 }).not.toBeNull();
  await page.evaluate(() => sessionStorage.setItem('quire_record_into_meeting', JSON.stringify({ id: 'meeting-other', title: 'Other' })));
  const args = await stopAndSave(page);
  expect(args.intoMeetingId ?? null).toBeNull();
});

test('coming back to a new meeting within the wait keeps it', async ({ page }) => {
  await open(page);
  await newMeeting(page);
  const title = String((await named(page, 'api_create_meeting'))[0][1].title);
  await page.getByRole('button', { name: 'Home' }).first().click();
  await page.waitForTimeout(300);
  await page.goBack();
  await expect(page.getByText('No recording yet')).toBeVisible();
  await page.waitForTimeout(3000);
  expect(await named(page, 'api_discard_empty_meeting')).toEqual([]);
  await expect(page.getByText(title).first()).toBeVisible();
});
