// Runs every custom lint check and fails if any of them fails.
// Add a check by appending to CHECKS. Usage: node scripts/lint/run.mjs
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FRONTEND = path.resolve(HERE, '..', '..');

const CHECKS = [
  { name: 'dashes', script: 'dashes.mjs' },
  { name: 'brand drift', script: '../brand-apply.mjs', args: ['--check'] },
  { name: 'brand mentions', script: 'brand.mjs' },
  { name: 'contrast', script: 'contrast.mjs' },
  { name: 'colors', script: 'colors.mjs' },
  { name: 'traceability', script: '../check-traceability.mjs' },
  { name: 'logs', script: 'logs.mjs' },
  { name: 'privacy', script: 'privacy.mjs' },
  { name: 'network', script: 'network.mjs' },
];

let failed = 0;
for (const check of CHECKS) {
  const r = spawnSync(process.execPath, [path.join(HERE, check.script), ...(check.args ?? [])], { cwd: FRONTEND, stdio: 'inherit' });
  if (r.status !== 0) failed += 1;
}
if (failed) {
  console.error(`lint: ${failed} of ${CHECKS.length} check(s) failed`);
  process.exit(1);
}
console.log(`lint: all ${CHECKS.length} check(s) passed`);
