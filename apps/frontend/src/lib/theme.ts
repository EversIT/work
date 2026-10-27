/**
 * Turns the administrator's accent colour into the HeroUI tokens that carry it. Everything in the
 * interface reads these tokens (`bg-accent`, `text-accent`, ...), so one colour re-themes it all.
 */

type Rgb = [number, number, number];

const hex = (v: number) =>
  Math.round(Math.min(255, Math.max(0, v)))
    .toString(16)
    .padStart(2, '0');
const toHex = ([r, g, b]: Rgb) => `#${hex(r)}${hex(g)}${hex(b)}`;

export function parseHex(color: string): Rgb | null {
  const m = /^#([0-9a-f]{6})$/i.exec(color.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const mix = (a: Rgb, b: Rgb, weightOfB: number): Rgb =>
  [0, 1, 2].map((i) => a[i] * (1 - weightOfB) + b[i] * weightOfB) as Rgb;

/** WCAG relative luminance, 0 (black) to 1 (white). */
export function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

const BLACK: Rgb = [0, 0, 0];

/** `null` for anything that is not a plain `#RRGGBB` colour, so a bad value never reaches a style. */
export function accentVars(color: string): Record<string, string> | null {
  const base = parseHex(color);
  if (!base) return null;
  const hover = mix(base, BLACK, 0.15);
  // Text on the accent: white unless the accent is so light that white would be hard to read.
  const foreground = luminance(base) > 0.45 ? '#172b4d' : '#ffffff';
  return {
    '--accent': toHex(base),
    '--accent-hover': toHex(hover),
    '--accent-foreground': foreground,
    '--accent-soft': `color-mix(in srgb, ${toHex(base)} 10%, var(--background))`,
    '--accent-soft-hover': `color-mix(in srgb, ${toHex(base)} 20%, var(--background))`,
    '--accent-soft-foreground': toHex(hover),
    '--field-border-focus': toHex(base),
    '--focus': toHex(base),
  };
}
