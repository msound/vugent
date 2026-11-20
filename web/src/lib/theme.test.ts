import { describe, expect, it } from 'vitest';
import { parseTheme } from './theme';

describe('parseTheme', () => {
  it('accepts the light and dark themes', () => {
    expect(parseTheme('light')).toBe('light');
    expect(parseTheme('dark')).toBe('dark');
  });

  it('falls back to auto for anything else', () => {
    expect(parseTheme('auto')).toBe('auto');
    expect(parseTheme(null)).toBe('auto');
    expect(parseTheme(undefined)).toBe('auto');
    expect(parseTheme('purple')).toBe('auto');
  });
});
