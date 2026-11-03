import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { findMentions } from '../../scripts/lint/brand.mjs';

describe('findMentions', () => {
  test("also finds the earlier product's company name, in any case", () => {
    const hits = findMentions('github.com/Zackriya-Solutions/x\nclean\nZACKRIYA');
    expect(hits.map((h: { line: number }) => h.line)).toEqual([1, 3]);
  });
});
