import { describe, expect, test } from 'bun:test';
import { resolve } from '../../src/contexts/ThemeContext';

describe('resolve', () => {
  test('System follows the operating system', () => {
    expect(resolve('system', true)).toBe('dark');
    expect(resolve('system', false)).toBe('light');
  });

  test('a forced choice ignores the operating system', () => {
    expect(resolve('light', true)).toBe('light');
    expect(resolve('dark', false)).toBe('dark');
  });
});
