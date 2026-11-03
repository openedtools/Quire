// Replaces pill shapes on controls and text badges with the token radii (A3 revision:
// "no rounded pill buttons"). Dots, avatars, spinners and progress bars stay round.
//   <button>, <Button>, <a> or anything with onClick: rounded-full -> rounded-control
//   a text badge (has horizontal padding and small text): rounded-full -> rounded-sm
// Usage: node scripts/codemod/square-corners.mjs [--write] <dir or file> ...
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const ts = require('typescript');

function* files(target) {
  if (statSync(target).isFile()) {
    if (target.endsWith('.tsx')) yield target;
    return;
  }
  for (const entry of readdirSync(target)) yield* files(path.join(target, entry));
}

/** Text edits [start, end, replacement] for one source file. */
export function edits(fileName, source) {
  const sf = ts.createSourceFile(fileName, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const out = [];
  const visit = (node) => {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      const tag = node.tagName.getText(sf);
      const attrs = node.attributes.properties.filter(ts.isJsxAttribute);
      const isControl = ['button', 'Button', 'a'].includes(tag) || attrs.some((a) => a.name.getText(sf) === 'onClick');
      const cls = attrs.find((a) => a.name.getText(sf) === 'className');
      if (cls?.initializer) {
        const text = cls.initializer.getText(sf);
        const isBadge = /\bpx-\d/.test(text) && /\btext-(xs|sm)\b/.test(text);
        const to = isControl ? 'rounded-control' : isBadge ? 'rounded-sm' : null;
        if (to) {
          const start = cls.initializer.getStart(sf);
          const re = /(?<![\w-])rounded-full(?![\w-])/g;
          for (const m of text.matchAll(re)) out.push([start + m.index, start + m.index + m[0].length, to]);
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return out;
}

const write = process.argv.includes('--write');
let total = 0;
for (const target of process.argv.slice(2).filter((a) => a !== '--write')) {
  for (const file of files(target)) {
    const source = readFileSync(file, 'utf8');
    const list = edits(file, source);
    if (!list.length) continue;
    total += list.length;
    let text = source;
    for (const [a, b, rep] of list.sort((x, y) => y[0] - x[0])) text = text.slice(0, a) + rep + text.slice(b);
    if (write) writeFileSync(file, text);
    console.log(`${file}: ${list.length}`);
  }
}
console.log(`${write ? 'changed' : 'would change'} ${total} pill shape(s)`);
