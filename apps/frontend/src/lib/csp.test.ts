import { describe, expect, it } from 'vitest';
import { buildCsp, newNonce } from './csp';

describe('buildCsp', () => {
  it('lets scripts run only with the nonce, never inline', () => {
    const csp = buildCsp('abc123', false);
    const script = csp.split('; ').find((d) => d.startsWith('script-src'))!;
    expect(script).toContain("'nonce-abc123'");
    expect(script).toContain("'strict-dynamic'");
    expect(script).not.toContain("'unsafe-inline'");
    expect(script).not.toContain("'unsafe-eval'");
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("frame-ancestors 'none'");
  });

  it('allows eval in development only (the dev server needs it)', () => {
    expect(buildCsp('n', true)).toContain("'unsafe-eval'");
  });
});

describe('newNonce', () => {
  it('is fresh every time and long enough', () => {
    const seen = new Set(Array.from({ length: 50 }, () => newNonce()));
    expect(seen.size).toBe(50);
    expect([...seen][0].length).toBeGreaterThanOrEqual(22);
  });
});
