import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';

// AC-UI-4: every control is reachable with Tab, shows a visible focus indicator,
// and is at least 24 by 24 px (WCAG 2.2 target size, minimum).
const MOCK = path.join(__dirname, 'mock-tauri.js');
const SCREENS = ['/', '/meeting-details?id=meeting-1', '/people', '/settings'];
const MIN = 24;

type Stop = { key: string; label: string; w: number; h: number; focusVisible: boolean };

async function open(page: Page, url: string) {
  await page.addInitScript({ path: MOCK });
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

/** The element Tab landed on, with its size and whether any focus styling is visible. */
async function current(page: Page): Promise<Stop | null> {
  return page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el || el === document.body) return null;
    el.dataset.tabbed = '1';
    const cs = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    const outline = cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0;
    const ring = cs.boxShadow !== 'none' && cs.boxShadow !== '';
    const label = (el.getAttribute('aria-label') || el.textContent || el.getAttribute('placeholder') || el.tagName).trim().slice(0, 40);
    const key = `${el.tagName}|${label}|${Math.round(r.x)},${Math.round(r.y)}`;
    return { key, label: `${el.tagName.toLowerCase()} "${label}"`, w: r.width, h: r.height, focusVisible: outline || ring };
  });
}

for (const url of SCREENS) {
  test(`keyboard: ${url}`, async ({ page }) => {
    await open(page, url);
    const show = page.getByRole('button', { name: 'Show meeting list' });
    if (await show.count()) await show.click();
    await page.getByRole('button', { name: 'Planning' }).first().click().catch(() => {});
    await page.mouse.move(0, 0);
    // Start from the top of the page, whatever took focus or was clicked while it loaded: the browser
    // resumes Tab from the last clicked element, so focus a temporary anchor at the very start
    await page.evaluate(() => {
      window.getSelection()?.removeAllRanges();
      const anchor = document.createElement('span');
      anchor.id = '__tab-start';
      anchor.tabIndex = -1;
      document.body.prepend(anchor);
      anchor.focus();
    });
    const seen = new Map<string, Stop>();
    for (let i = 0; i < 200; i += 1) {
      await page.keyboard.press('Tab');
      await page.evaluate(() => document.getElementById('__tab-start')?.remove());
      const stop = await current(page);
      if (!stop) continue;
      if (seen.has(stop.key)) break; // wrapped around
      seen.set(stop.key, stop);
      // Editors keep Tab for indenting; Escape leaves them, as the hint tells screen reader users
      if (await page.evaluate(() => (document.activeElement as HTMLElement | null)?.isContentEditable)) {
        await page.keyboard.press('Escape');
      }
    }
    const stops = [...seen.values()];
    const noFocus = stops.filter((s) => !s.focusVisible).map((s) => s.label);
    const small = stops.filter((s) => s.w < MIN || s.h < MIN).map((s) => `${s.label} ${Math.round(s.w)}x${Math.round(s.h)}`);

    // Visible, tabbable controls that Tab never reached
    const reachable = await page.evaluate(() => {
      // Roving-focus widgets (tab lists, radio groups) keep inactive items at tabindex -1 on purpose:
      // they are reached with the arrow keys, and the container hands focus to the active item,
      // so neither is counted here.
      const sel = 'button:not([disabled]), a[href], input:not([disabled]):not([type=hidden]), select:not([disabled]), textarea:not([disabled]), [tabindex]';
      return Array.from(document.querySelectorAll<HTMLElement>(sel))
        .filter((el) => {
          const r = el.getBoundingClientRect();
          const container = ['tablist', 'radiogroup', 'menubar', 'toolbar'].includes(el.getAttribute('role') ?? '');
          return !container && el.tabIndex >= 0 && !el.dataset.tabbed && r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
        })
        .map((el) => `${el.tagName.toLowerCase()} "${(el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 30)}"`);
    });

    const missing = reachable;

    // Clickable things that are not focusable at all (a div with onClick and a pointer cursor),
    // and buttons a screen reader would only announce as "button"
    const unreachableClickable = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('.cursor-pointer'))
        .filter((el) => {
          const r = el.getBoundingClientRect();
          const native = el.matches('button, a[href], input, select, textarea, summary, label');
          return !native && el.tabIndex < 0 && r.width > 0 && r.height > 0 && !el.closest('button, a[href], [role="button"][tabindex]');
        })
        .map((el) => `${el.tagName.toLowerCase()} "${(el.textContent || '').trim().slice(0, 30)}"`));
    const unnamed = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('button, [role="button"]'))
        .filter((el) => {
          const r = el.getBoundingClientRect();
          const name = (el.getAttribute('aria-label') || el.getAttribute('aria-labelledby') || el.textContent || '').trim();
          return r.width > 0 && r.height > 0 && !name;
        })
        .map((el) => el.outerHTML.slice(0, 80)));
    console.log(`${url}: ${stops.length} tab stops`);
    if (process.env.KB_DEBUG) console.log(stops.map((s) => s.label).join(" > "));
    if (missing.length) console.log(`  not reached by Tab: ${missing.join('; ')}`);
    if (noFocus.length) console.log(`  no visible focus: ${noFocus.join('; ')}`);
    if (small.length) console.log(`  smaller than ${MIN}px: ${small.join('; ')}`);
    expect(noFocus, 'every control shows a focus indicator').toEqual([]);
    expect(small, `every control is at least ${MIN} by ${MIN} px`).toEqual([]);
    if (unreachableClickable.length) console.log(`  clickable but not focusable: ${unreachableClickable.join('; ')}`);
    if (unnamed.length) console.log(`  buttons without a name: ${unnamed.join('; ')}`);
    expect(missing, 'Tab reaches every visible control').toEqual([]);
    expect(unreachableClickable, 'everything clickable can be reached by keyboard').toEqual([]);
    expect(unnamed, 'every button has an accessible name').toEqual([]);
  });
}
