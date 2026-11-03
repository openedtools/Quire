import { describe, expect, test } from 'bun:test';
import { isKeptCitation, linkCitations, noteBlockMarkdown, parseJumpHref, segmentAtTime } from '../../src/lib/ask';

const segments = [
  { id: 'a', timestamp: 0, endTime: 9 },
  { id: 'b', timestamp: 125.6, endTime: 131 },
  { id: 'c', timestamp: 3723, endTime: 3730 },
];

describe('Ask answers', () => {
  test('kept citations become jump links and others stay as text', () => {
    const answer = 'March [02:05], see [1:02:04] and [03:00].';
    const linked = linkCitations(answer, [
      { label: '02:05', seconds: 125 },
      { label: '1:02:04', seconds: 3724 },
    ]);
    expect(linked).toBe('March [02:05](#t=125), see [1:02:04](#t=3724) and [03:00].');
  });

  test('a citation already followed by a link is not linked twice', () => {
    expect(linkCitations('[02:05](x)', [{ label: '02:05', seconds: 125 }])).toBe('[02:05](x)');
  });

  test('jump links are read back and anything else is not a jump', () => {
    expect(parseJumpHref('#t=125')).toBe(125);
    expect(parseJumpHref('#t=3724.5')).toBe(3724.5);
    expect(parseJumpHref('https://example.com')).toBeNull();
    expect(parseJumpHref('#t=abc')).toBeNull();
    expect(parseJumpHref(undefined)).toBeNull();
  });

  test('a time finds the line that contains it, using the shown start time', () => {
    expect(segmentAtTime(segments, 125)).toBe('b');
    expect(segmentAtTime(segments, 3724)).toBe('c');
    expect(segmentAtTime(segments, 60)).toBe('a');
    expect(segmentAtTime([], 5)).toBeUndefined();
  });

  test('an answer added to notes is labelled as AI generated with its question', () => {
    const md = noteBlockMarkdown('What did I miss?', 'Jo moved the date [02:05].');
    expect(md).toBe('**Ask (AI generated):** What did I miss?\n\nJo moved the date [02:05].');
  });
});

describe('Ask review fixes', () => {
  test('only a kept citation can jump', () => {
    const kept = [{ label: '02:05', seconds: 125 }];
    expect(isKeptCitation(125, kept)).toBe(true);
    expect(isKeptCitation(9999, kept)).toBe(false);
  });

  test('Add to note keeps link text but drops links, images and raw HTML', () => {
    const md = noteBlockMarkdown('q', 'See [the site](https://example.com) ![x](https://example.com/i.png) <b>now</b> [02:05](#t=125)');
    expect(md).toBe('**Ask (AI generated):** q\n\nSee the site  now [02:05]');
  });
});
