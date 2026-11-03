// Generates raster brand assets from the artwork the owner supplied in brand/source (spec F-UI-9,
// plan WP7). The artwork is used as given: only its format and size change.
//   brand/source/mark.webp       transparent mark: sidebar, About, installer images
//   brand/source/app-icon.webp   the mark on a tile: app, taskbar, tray and .exe icon
// Outputs:
//   brand/generated/app-icon.png        input for `pnpm tauri icon` (all sizes and the .ico)
//   brand/generated/preview-<n>.png     the app icon at 16, 32, 64 and 256 px (AC-UI-6)
//   public/mark.png                     the mark inside the app (512 px)
//   src-tauri/icons/installer/header.bmp    150 x 57, Windows installer header
//   src-tauri/icons/installer/sidebar.bmp   164 x 314, Windows installer welcome and finish pages
// Usage: node scripts/brand-assets.mjs   (uses Playwright's Chromium; no network)
import { chromium } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const FRONTEND = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const BRAND = path.resolve(FRONTEND, '..', 'brand');
const GENERATED = path.join(BRAND, 'generated');
const INSTALLER = path.join(FRONTEND, 'src-tauri', 'icons', 'installer');
const dataUrl = (file) => `data:image/webp;base64,${readFileSync(path.join(BRAND, 'source', file)).toString('base64')}`;
const MARK = dataUrl('mark.webp');
const APP_ICON = dataUrl('app-icon.webp');
const PAPER = '#FAF6EE';

/** 24-bit bottom-up BMP from RGBA pixels (what NSIS expects). */
export function encodeBmp(width, height, rgba) {
  const rowSize = Math.ceil((width * 3) / 4) * 4;
  const size = 54 + rowSize * height;
  const buf = Buffer.alloc(size);
  buf.write('BM', 0);
  buf.writeUInt32LE(size, 2);
  buf.writeUInt32LE(54, 10);
  buf.writeUInt32LE(40, 14);
  buf.writeInt32LE(width, 18);
  buf.writeInt32LE(height, 22);
  buf.writeUInt16LE(1, 26);
  buf.writeUInt16LE(24, 28);
  buf.writeUInt32LE(rowSize * height, 34);
  for (let y = 0; y < height; y += 1) {
    const row = 54 + (height - 1 - y) * rowSize;
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 4;
      buf[row + x * 3] = rgba[i + 2];
      buf[row + x * 3 + 1] = rgba[i + 1];
      buf[row + x * 3 + 2] = rgba[i];
    }
  }
  return buf;
}

async function render(page, html, width, height) {
  await page.setViewportSize({ width, height });
  await page.setContent(`<html><body style="margin:0;width:${width}px;height:${height}px;overflow:hidden;background:transparent">${html}</body></html>`);
  await page.evaluate(() => Promise.all(Array.from(document.images).map((img) => img.decode())));
  return page;
}

async function pixels(page, width, height) {
  const png = await page.screenshot({ clip: { x: 0, y: 0, width, height } });
  return page.evaluate(async ({ b64, width, height }) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(img, 0, 0);
    return Array.from(ctx.getImageData(0, 0, width, height).data);
  }, { b64: png.toString('base64'), width, height });
}

const img = (src, size, style = '') => `<img src="${src}" width="${size}" height="${size}" style="display:block;${style}">`;

async function main() {
  mkdirSync(GENERATED, { recursive: true });
  mkdirSync(INSTALLER, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage();

  await render(page, img(APP_ICON, 1024), 1024, 1024);
  await page.screenshot({ path: path.join(GENERATED, 'app-icon.png'), omitBackground: true });
  for (const n of [16, 32, 64, 256]) {
    await render(page, img(APP_ICON, n), n, n);
    await page.screenshot({ path: path.join(GENERATED, `preview-${n}.png`), omitBackground: true });
  }

  await render(page, img(MARK, 512), 512, 512);
  await page.screenshot({ path: path.join(FRONTEND, 'public', 'mark.png'), omitBackground: true });

  // Header: cream with the mark at the right, no text (NSIS draws its own title)
  await render(page, `<div style="width:150px;height:57px;background:${PAPER};position:relative">
    <div style="position:absolute;right:6px;top:3px">${img(MARK, 51)}</div></div>`, 150, 57);
  writeFileSync(path.join(INSTALLER, 'header.bmp'), encodeBmp(150, 57, await pixels(page, 150, 57)));

  // Sidebar: cream with the mark in the lower half and generous empty space above
  await render(page, `<div style="width:164px;height:314px;background:${PAPER};position:relative">
    <div style="position:absolute;left:22px;bottom:40px">${img(MARK, 120)}</div></div>`, 164, 314);
  writeFileSync(path.join(INSTALLER, 'sidebar.bmp'), encodeBmp(164, 314, await pixels(page, 164, 314)));

  await browser.close();
  console.log('brand assets written: app-icon.png, previews, public/mark.png, installer header and sidebar');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) await main();
