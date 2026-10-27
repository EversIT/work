import { describe, expect, it } from 'vitest';
import { accentVars, luminance, parseHex } from './theme';

describe('accentVars', () => {
  it('derives the accent tokens from one colour', () => {
    const v = accentVars('#0C66E4')!;
    expect(v['--accent']).toBe('#0c66e4');
    expect(v['--accent-foreground']).toBe('#ffffff');
    // hover is darker, the soft tones are lighter than the accent itself
    expect(luminance(parseHex(v['--accent-hover'])!)).toBeLessThan(
      luminance(parseHex(v['--accent'])!),
    );
    expect(v['--accent-soft']).toContain('#0c66e4');
    expect(v['--accent-soft']).toContain('var(--background)');
    expect(v['--accent-soft-hover']).toContain('20%');
    expect(v['--accent-soft-foreground']).toBe(v['--accent-hover']);
  });

  it('uses dark text on a light accent so it stays readable', () => {
    expect(accentVars('#ffe380')!['--accent-foreground']).toBe('#172b4d');
    expect(accentVars('#000000')!['--accent-foreground']).toBe('#ffffff');
  });

  it('refuses anything that is not a plain #RRGGBB colour', () => {
    for (const bad of [
      'red',
      '#12345',
      '#1234567',
      'url(javascript:alert(1))',
      '#0c66e4; color: red',
      '',
      'rgb(1,2,3)',
    ]) {
      expect(accentVars(bad)).toBeNull();
    }
  });
});
