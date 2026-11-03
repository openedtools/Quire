import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { checkTraceability, criterionIds, requirementIds, traceRows } from '../../scripts/check-traceability.mjs';

const SPEC = [
  '1. **F-UI-1 (M)** Tokens.',
  '2. **F-UI-2 (S)** Themes.',
  '1. **NF-1 Offline by default.** No network.',
  '| AC-UI-1 | Contrast | lint | pass |',
  '| AC-NF-1 | Offline | test | pass |',
  '| F-UI-1, F-UI-2 | AC-UI-1 |',
  '| NF-1 | AC-NF-1 |',
].join('\r\n');

describe('traceability', () => {
  test('reads requirements, criteria and table rows, with Windows line endings', () => {
    expect(requirementIds(SPEC)).toEqual(['F-UI-1', 'F-UI-2', 'NF-1']);
    expect(criterionIds(SPEC)).toEqual(['AC-UI-1', 'AC-NF-1']);
    expect(traceRows(SPEC)).toEqual([
      { requirements: ['F-UI-1', 'F-UI-2'], criteria: ['AC-UI-1'] },
      { requirements: ['NF-1'], criteria: ['AC-NF-1'] },
    ]);
    expect(checkTraceability(SPEC)).toEqual([]);
  });

  test('an untraced requirement, an unknown criterion and an unknown requirement all fail', () => {
    const spec = SPEC.replace('| NF-1 | AC-NF-1 |', '| NF-1, F-XX-9 | AC-NF-7 |') + '\r\n3. **F-UI-3 (M)** Contrast.';
    expect(checkTraceability(spec)).toEqual([
      'F-UI-3 has no acceptance criterion in the traceability table',
      'traceability table names unknown requirement F-XX-9',
      'traceability table names unknown criterion AC-NF-7',
    ]);
  });
});
