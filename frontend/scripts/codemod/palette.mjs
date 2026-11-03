// Rewrites raw Tailwind palette classes (gray-600, blue-50, red-700, white ...) into the
// semantic token classes from tailwind.config.js (spec F-UI-7, plan WP6).
// Usage: node scripts/codemod/palette.mjs [--write] <file or directory> ...
// Without --write it only reports. It always lists what it could not map.
import { readFileSync, writeFileSync, statSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const FAMILIES = 'gray|slate|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|white|black';
const UTILITIES = 'bg|text|border(?:-[trblxy])?|ring|ring-offset|from|via|to|fill|stroke|divide|outline|placeholder|shadow|decoration|caret';

/** A class with optional variants (hover:, md:, group-hover: ...), utility, colour, shade and opacity. */
export const RAW = new RegExp(
  `(?<![\\w-])((?:[\\w-]+:)*)(${UTILITIES})-(${FAMILIES})(?:-(\\d{2,3}))?(\\/\\d+)?(?![\\w-])`,
  'g',
);

const NEUTRAL = ['gray', 'slate', 'zinc', 'neutral', 'stone'];
const ACCENT = ['blue', 'indigo', 'sky', 'cyan', 'violet', 'purple', 'fuchsia', 'teal'];
const DANGER = ['red', 'rose', 'pink'];
const SUCCESS = ['green', 'emerald', 'lime'];
const WARNING = ['yellow', 'amber', 'orange'];

const isState = (variants) => /(?:^|:)(hover|focus|focus-visible|focus-within|active|group-hover|data-\[state=open\]|data-\[highlighted\]|aria-selected):$/.test(variants);

/** The semantic colour for one utility, colour and shade, or null when unmapped. */
export function mapColour(utility, family, shade, variants = '') {
  const n = shade ? Number(shade) : null;
  const u = utility.startsWith('border') ? 'border' : utility;
  const state = isState(variants);

  if (family === 'white') {
    if (u === 'text' || u === 'fill' || u === 'stroke') return 'on-accent';
    return 'paper';
  }
  if (family === 'black') {
    if (u === 'bg') return 'scrim';
    return 'ink';
  }

  if (NEUTRAL.includes(family)) {
    if (u === 'text' || u === 'fill' || u === 'stroke' || u === 'placeholder' || u === 'decoration' || u === 'caret') {
      return n >= 700 ? 'ink' : 'ink-muted';
    }
    if (u === 'bg') {
      if (n <= 100) return state ? 'surface-hover' : 'surface';
      if (n === 200) return 'surface-hover';
      if (n === 300) return 'line';
      if (n <= 500) return 'line-strong';
      return 'ink';
    }
    if (u === 'border' || u === 'divide' || u === 'outline') {
      if (n <= 200) return 'line';
      if (n <= 500) return 'line-strong';
      return 'ink';
    }
    if (u === 'ring' || u === 'ring-offset') return 'line-strong';
    if (u === 'from' || u === 'via' || u === 'to') return n <= 200 ? 'surface' : 'ink';
    return null;
  }

  const tone = ACCENT.includes(family) ? 'accent' : DANGER.includes(family) ? 'danger'
    : SUCCESS.includes(family) ? 'success' : WARNING.includes(family) ? 'warning' : null;
  if (!tone) return null;

  if (u === 'bg' || u === 'from' || u === 'via' || u === 'to') {
    if (n <= 300) return `${tone}-tint`;
    // A hover or pressed shade of a filled button: the same fill, slightly see-through
    if (state && tone !== 'accent' && n >= 600) return `${tone}/90`;
    if (tone === 'accent') {
      if (n === 400) return 'accent';
      if (n >= 800) return 'accent-strong-pressed';
      if (n === 700) return 'accent-strong-hover';
      return 'accent-strong';
    }
    return tone;
  }
  if (u === 'text' || u === 'fill' || u === 'stroke' || u === 'placeholder' || u === 'decoration') {
    return tone === 'accent' ? 'accent-strong' : tone;
  }
  if (u === 'border' || u === 'divide' || u === 'outline' || u === 'ring' || u === 'ring-offset') {
    if (n <= 300) return `${tone}/40`;
    return tone === 'accent' ? 'accent' : tone;
  }
  return null;
}

/** Rewrites one source text. Returns { text, changed, unmapped: string[] }. */
export function rewrite(source) {
  let changed = 0;
  const unmapped = [];
  const text = source.replace(RAW, (whole, variants, utility, family, shade, opacity) => {
    const colour = mapColour(utility, family, shade, variants);
    if (!colour) {
      unmapped.push(whole);
      return whole;
    }
    // A mapped colour that already carries an opacity (danger/40) keeps it; otherwise keep the source's
    const out = colour.includes('/') ? `${variants}${utility}-${colour}` : `${variants}${utility}-${colour}${opacity ?? ''}`;
    changed += 1;
    return out;
  }).replace(SHADOW, () => {
    changed += 1;
    return '';
  });
  return { text, changed, unmapped };
}

/** Drop shadows are not part of the look (spec F-UI-5); separation comes from surface and line. */
export const SHADOW = /(?<![\w-])((?:[\w-]+:)*)shadow(?:-(?:sm|md|lg|xl|2xl|inner))?(?![\w\/-])\s?/g;

/** Hex colours in source (className arbitrary values, inline styles). Reported, never rewritten. */
export function findHex(source) {
  return [...source.matchAll(/#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?(?:[0-9a-fA-F]{2})?\b/g)].map((m) => m[0]);
}

function* files(target) {
  const stat = statSync(target);
  if (stat.isFile()) {
    if (/\.(tsx?|jsx?)$/.test(target)) yield target;
    return;
  }
  for (const entry of readdirSync(target)) yield* files(path.join(target, entry));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const write = process.argv.includes('--write');
  const targets = process.argv.slice(2).filter((a) => a !== '--write');
  let total = 0;
  for (const target of targets) {
    for (const file of files(target)) {
      const source = readFileSync(file, 'utf8');
      const { text, changed, unmapped } = rewrite(source);
      const hex = findHex(source);
      if (!changed && !unmapped.length && !hex.length) continue;
      total += changed;
      console.log(`${file}: ${changed} rewritten${unmapped.length ? `, unmapped: ${unmapped.join(' ')}` : ''}${hex.length ? `, hex: ${hex.join(' ')}` : ''}`);
      if (write && changed) writeFileSync(file, text);
    }
  }
  console.log(`${write ? 'rewrote' : 'would rewrite'} ${total} class(es)`);
}
