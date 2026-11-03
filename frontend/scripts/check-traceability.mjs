// Fails when a requirement in spec.md section 2 has no acceptance criterion in the
// traceability table (section 4), or when the table names a requirement or criterion
// that does not exist. Usage: node scripts/check-traceability.mjs
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SPEC = path.resolve(HERE, '..', '..', 'intent', 'private-meeting-recorder', 'spec.md');

/** Requirement ids defined in the spec, such as F-ID-1 or NF-3. */
export function requirementIds(spec) {
  // Functional: **F-UI-3 (M)**. Non-functional: **NF-2 Accessibility.**
  return [...new Set([...spec.matchAll(/\*\*(F-[A-Z]+-\d+) \((?:M|S|C)\)\*\*|\*\*(NF-\d+) [^*]+\*\*/g)].map((m) => m[1] ?? m[2]))];
}

/** Criterion ids defined as rows of the acceptance criteria table. */
export function criterionIds(spec) {
  return [...new Set([...spec.matchAll(/^\| (AC-[A-Z]+-\d+) \|/gm)].map((m) => m[1]))];
}

/** Rows of the traceability table: [{ requirements: [...], criteria: [...] }]. */
export function traceRows(spec) {
  const ids = (text) => [...text.matchAll(/(?:F-[A-Z]+|NF|AC-[A-Z]+)-\d+/g)].map((x) => x[0]);
  return [...spec.matchAll(/^\| ((?:(?:F-[A-Z]+|NF)-\d+(?:, )?)+) \| ([^|]+) \|\r?$/gm)]
    .map((m) => ({ requirements: ids(m[1]), criteria: ids(m[2]) }));
}

/** Returns failure messages. */
export function checkTraceability(spec) {
  const reqs = requirementIds(spec);
  const acs = new Set(criterionIds(spec));
  const rows = traceRows(spec);
  const traced = new Set(rows.flatMap((r) => r.requirements));
  const failures = [];
  for (const r of reqs) if (!traced.has(r)) failures.push(`${r} has no acceptance criterion in the traceability table`);
  for (const row of rows) {
    for (const r of row.requirements) if (!reqs.includes(r)) failures.push(`traceability table names unknown requirement ${r}`);
    for (const c of row.criteria) if (!acs.has(c)) failures.push(`traceability table names unknown criterion ${c}`);
    if (!row.criteria.length) failures.push(`${row.requirements.join(', ')} maps to no criterion`);
  }
  return failures;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const spec = readFileSync(SPEC, 'utf8');
  const failures = checkTraceability(spec);
  if (failures.length) {
    console.error('traceability: FAILED');
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`traceability: ok (${requirementIds(spec).length} requirements, ${criterionIds(spec).length} criteria)`);
}
