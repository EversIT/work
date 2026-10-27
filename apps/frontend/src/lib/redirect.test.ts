import { beforeEach, describe, expect, it } from 'vitest';
import { rememberNext, safeNext, takePendingNext } from './redirect';

describe('safeNext', () => {
  it('only lets same-site paths through', () => {
    expect(safeNext('/invite?token=abc')).toBe('/invite?token=abc');
    for (const bad of [
      '//evil.test',
      'https://evil.test',
      '/\\evil.test',
      'relative',
      '',
      null,
      undefined,
      '/a\nb',
    ]) {
      expect(safeNext(bad)).toBe('/');
    }
  });
});

describe('pending target across e-mail confirmation', () => {
  beforeEach(() => localStorage.clear());

  it('is kept once and then forgotten', () => {
    rememberNext('/invite?token=abc');
    expect(takePendingNext()).toBe('/invite?token=abc');
    expect(takePendingNext()).toBe('/');
  });

  it('forgets a target that is older than the confirmation link', () => {
    localStorage.setItem(
      'work.pendingNext',
      JSON.stringify({ next: '/old', at: Date.now() - 25 * 3600_000 }),
    );
    expect(takePendingNext()).toBe('/');
  });

  it('never trusts what is in storage', () => {
    localStorage.setItem(
      'work.pendingNext',
      JSON.stringify({ next: 'https://evil.test', at: Date.now() }),
    );
    expect(takePendingNext()).toBe('/');
    localStorage.setItem('work.pendingNext', 'not json');
    expect(takePendingNext()).toBe('/');
  });

  it('does not store the home page', () => {
    rememberNext('/');
    expect(localStorage.getItem('work.pendingNext')).toBeNull();
  });
});
