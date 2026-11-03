import { describe, expect, test } from 'bun:test';
import { contentWords, isFromNotes, noteLinesOf } from '../../src/lib/noteLines';

const NOTES = [
  '- Budget for venue is fixed at 4k',
  '- Sam drafts board note by Friday',
  'ok',
  '',
].join('\n');

describe('note lines', () => {
  test('Markdown marks, punctuation and common words are ignored', () => {
    expect([...contentWords('- The **budget** is fixed!')]).toEqual(['budget', 'fixed']);
    expect([...contentWords('## Action items')]).toEqual(['action', 'items']);
  });

  test('only lines with at least two content words are matched', () => {
    expect(noteLinesOf(NOTES)).toHaveLength(2);
  });

  test('a summary line that restates a note counts as from the notes', () => {
    const notes = noteLinesOf(NOTES);
    expect(isFromNotes('The venue budget is fixed at 4k.', notes)).toBe(true);
    expect(isFromNotes('- Sam will draft the board note by Friday', notes)).toBe(true);
  });

  test('a line the model added from the transcript does not', () => {
    const notes = noteLinesOf(NOTES);
    expect(isFromNotes('Jordan confirms the venue dates this afternoon.', notes)).toBe(false);
    expect(isFromNotes('Fixed', notes)).toBe(false);
  });

  test('no notes means nothing is from the notes', () => {
    expect(isFromNotes('The venue budget is fixed', noteLinesOf(''))).toBe(false);
  });
});
