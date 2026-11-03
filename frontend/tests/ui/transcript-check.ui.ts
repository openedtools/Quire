import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// Line editing and the transcript check (plan PL5), against mock-tauri.js's made-up meeting plus
// stand-ins for the Phase L commands. Nothing changes without a click.
const OUT = path.join(__dirname, 'screenshots');

async function open(page: Page) {
  await page.addInitScript({ path: path.join(__dirname, 'mock-tauri.js') });
  await page.addInitScript(() => {
    const w = window as unknown as {
      __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> };
      __phaseL: { calls: [string, Record<string, unknown> | undefined][]; edits: { lineId: string; original: string }[] };
    };
    const base = w.__TAURI_INTERNALS__.invoke.bind(w.__TAURI_INTERNALS__);
    w.__phaseL = { calls: [], edits: [] };
    const extra: Record<string, (a?: Record<string, unknown>) => unknown> = {
      api_transcript_edits: () => w.__phaseL.edits,
      api_edit_transcript_line: (a) => { w.__phaseL.edits.push({ lineId: String(a?.lineId), original: 'before' }); return null; },
      api_replace_in_transcript: () => 2,
      api_apply_suggestion: (a) => `applied ${String(a?.replace)}`,
      api_check_transcript: () => [
        { lineId: 't1', start: 0, lineText: 'Thanks for joining. The goal today is to agree the plan for next month.', find: 'agree', replace: 'agree on', reason: 'missing word' },
        { lineId: 't2', start: 9, lineText: 'I looked at the numbers.', find: 'numbers', replace: 'figures', reason: 'unclear word' },
        { lineId: 't3', start: 19, lineText: 'Sam drafts the numbers note.', find: 'numbers', replace: 'figures', reason: 'unclear word' },
      ],
    };
    w.__TAURI_INTERNALS__.invoke = (c, a) => {
      if (c in extra) {
        w.__phaseL.calls.push([c, a]);
        return Promise.resolve(extra[c](a));
      }
      return base(c, a);
    };
  });
  await page.goto('/meeting-details?id=meeting-1');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

const calls = (page: Page) => page.evaluate(() => (window as unknown as { __phaseL: { calls: [string, unknown][] } }).__phaseL.calls.map(([c]) => c));

test('a line can be edited, and Escape leaves it as it was', async ({ page }) => {
  await open(page);
  const edit = page.getByRole('button', { name: 'Edit the line at [00:00]' });
  await edit.focus();
  await edit.press('Enter');
  const box = page.getByRole('textbox', { name: 'Text of the line at [00:00]' });
  await expect(box).toBeVisible();
  await box.press('Escape');
  expect(await calls(page)).not.toContain('api_edit_transcript_line');
  await edit.click();
  await box.fill('Thanks for joining. The goal today is to agree on the plan.');
  await page.getByRole('button', { name: 'Save the line at [00:00]' }).click();
  expect(await calls(page)).toContain('api_edit_transcript_line');
  await expect(page.getByRole('button', { name: 'Undo the change to the line at [00:00]' })).toBeVisible();
});

test('the check only changes what is accepted', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Check' }).click();
  const dialog = page.getByRole('dialog', { name: 'Check transcript' });
  await expect(dialog.getByText('missing word')).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(OUT, `25-check-transcript-${test.info().project.name}.png`) });
  expect(await calls(page)).not.toContain('api_apply_suggestion');
  await dialog.getByRole('button', { name: 'Skip the suggestion for agree' }).click();
  await expect(dialog.getByText('missing word')).toHaveCount(0);
  await dialog.getByRole('button', { name: /Accept all 2/ }).first().click();
  expect(await calls(page)).toContain('api_replace_in_transcript');
  await expect(dialog.getByText('No likely mistakes found.')).toBeVisible();
  expect(await calls(page)).not.toContain('api_apply_suggestion');
});
