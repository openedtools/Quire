import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// The iPhone's look on the desktop (plan PL6): the home landscape, the recording view with its
// waveform and hills, the meeting's pill tabs and Copy buttons, and Settings, Models with the
// new sections. Made-up data from mock-tauri.js plus the commands added in Phase L.
const OUT = path.join(__dirname, 'screenshots');
const MOCK = path.join(__dirname, 'mock-tauri.js');

async function open(page: Page, url: string) {
  await page.addInitScript({ path: MOCK });
  await page.addInitScript(() => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__;
    const base = internals.invoke.bind(internals);
    let t = 0;
    let recording = false;
    const state = () => ({ is_recording: recording, is_paused: false, is_active: recording, recording_duration: recording ? 754 : null, active_duration: recording ? 754 : null });
    const extra: Record<string, () => unknown> = {
      get_recording_state: state,
      is_recording: () => recording,
      api_recording_level: () => 0.25 + 0.35 * Math.abs(Math.sin((t += 0.45))),
      api_naming_get_settings: () => ({ autoName: true, titleFormat: '{date} {topic}' }),
      api_model_guide: () => ({
        machine: { appleSilicon: false, memoryGb: 16, cores: 8, nvidia: true },
        recommendation: {
          model: 'qwen3.5:2b',
          label: 'Qwen 3.5 2B (Balanced)',
          reason: 'The built-in engine uses the processor here. With 16 GB of memory and 8 cores the balanced model is usable.',
          otherOption: 'This computer has an NVIDIA graphics card. Ollama and LM Studio can use it.',
        },
      }),
    };
    internals.invoke = (command, args) => {
      if (command === 'start_recording_with_devices_and_meeting') recording = true;
      return command in extra ? Promise.resolve(extra[command]()) : base(command, args);
    };
  });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

async function shot(page: Page, name: string) {
  await page.screenshot({ path: path.join(OUT, `${name}-${test.info().project.name}.png`) });
}

test('home with the landscape', async ({ page }) => {
  await open(page, '/');
  await expect(page.getByRole('heading', { name: /Welcome to/ })).toBeVisible();
  await shot(page, '20-look-home');
});

test('recording with the waveform and hills', async ({ page }) => {
  await open(page, '/');
  await page.getByRole('main').getByRole('button', { name: 'Start recording' }).click();
  const gotIt = page.getByRole('button', { name: 'Got it' });
  if (await gotIt.isVisible().catch(() => false)) await gotIt.click();
  await page.mouse.move(900, 400);
  await page.waitForTimeout(2500);
  await shot(page, '21-look-recording');
});

test('meeting with pill tabs and Copy', async ({ page }) => {
  await open(page, '/meeting-details?id=meeting-1');
  await page.getByRole('tab', { name: 'My notes' }).click();
  await expect(page.getByRole('button', { name: 'Copy the notes' })).toBeVisible();
  await page.mouse.move(900, 500);
  await page.waitForTimeout(500);
  await shot(page, '22-look-meeting-notes');
});

test('settings, models with names and the guide', async ({ page }) => {
  await open(page, '/settings?tab=models');
  await page.getByRole('tab', { name: /Models/ }).click().catch(() => {});
  await page.waitForTimeout(500);
  await expect(page.getByText('Which model suits this computer')).toBeVisible();
  await expect(page.getByText('Meeting names')).toBeVisible();
  await shot(page, '23-look-models');
  await page.getByText('Which model suits this computer').scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await shot(page, '24-look-model-guide');
});
