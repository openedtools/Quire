import { describe, expect, test } from 'bun:test';
import { duration } from '../../src/components/ModelGuide';

describe('Model guide', () => {
  test('durations read as plain words', () => {
    expect(duration(0.4)).toBe('1 second');
    expect(duration(40)).toBe('40 seconds');
    expect(duration(95)).toBe('about 2 minutes');
    expect(duration(3600)).toBe('about 60 minutes');
  });
});
