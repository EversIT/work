import { describe, expect, it } from 'vitest';
import { GAP_MS, sinceLastVisit } from './visit';

const fakeStore = () => {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
  };
};
const at = (iso: string) => new Date(iso);

describe('sinceLastVisit', () => {
  it('covers a week on the very first visit', () => {
    expect(
      sinceLastVisit(
        fakeStore(),
        'w',
        at('2026-10-28T10:00:00Z'),
      ).toISOString(),
    ).toBe('2026-10-21T10:00:00.000Z');
  });

  it('after a pause, the previous visit is what to catch up on', () => {
    const s = fakeStore();
    sinceLastVisit(s, 'w', at('2026-10-27T09:00:00Z')); // first visit
    const since = sinceLastVisit(s, 'w', at('2026-10-28T10:00:00Z')); // next morning
    expect(since.toISOString()).toBe('2026-10-27T09:00:00.000Z');
  });

  it('keeps the summary stable while the same visit goes on, reloads included', () => {
    const s = fakeStore();
    sinceLastVisit(s, 'w', at('2026-10-27T09:00:00Z'));
    const first = sinceLastVisit(s, 'w', at('2026-10-28T10:00:00Z'));
    const reload = sinceLastVisit(
      s,
      'w',
      new Date(at('2026-10-28T10:00:00Z').getTime() + GAP_MS - 1000),
    );
    expect(reload.toISOString()).toBe(first.toISOString());
  });

  it('keeps workspaces apart, and never reaches back more than 30 days', () => {
    const s = fakeStore();
    sinceLastVisit(s, 'a', at('2026-08-01T09:00:00Z'));
    expect(
      sinceLastVisit(s, 'b', at('2026-10-28T10:00:00Z')).toISOString(),
    ).toBe('2026-10-21T10:00:00.000Z');
    expect(
      sinceLastVisit(s, 'a', at('2026-10-28T10:00:00Z')).toISOString(),
    ).toBe('2026-09-28T10:00:00.000Z');
  });

  it('works without storage, and when storage throws', () => {
    expect(
      sinceLastVisit(null, 'w', at('2026-10-28T10:00:00Z')).toISOString(),
    ).toBe('2026-10-21T10:00:00.000Z');
    const broken = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(
      sinceLastVisit(broken, 'w', at('2026-10-28T10:00:00Z')).toISOString(),
    ).toBe('2026-10-21T10:00:00.000Z');
  });
});
