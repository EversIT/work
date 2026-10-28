import { describe, expect, it } from 'vitest';
import {
  FIELD_TEMPLATES,
  label,
  rankTemplates,
  STATUS_TEMPLATES,
} from './templates';

const ids = (xs: { id: string }[]) => xs.map((x) => x.id);

describe('rankTemplates', () => {
  it('finds the support workflow for the example in the hook', () => {
    const r = rankTemplates(
      STATUS_TEMPLATES,
      'support team with triage and customer feedback',
      ['simple'],
    );
    expect(ids(r)[0]).toBe('support');
  });

  it('finds the fields for a bug tracker of a mobile app, best first', () => {
    const r = rankTemplates(FIELD_TEMPLATES, 'bug tracker for a mobile app');
    expect(ids(r).slice(0, 2).sort()).toEqual(['bugs', 'mobile']);
  });

  it('reads German', () => {
    expect(
      ids(
        rankTemplates(STATUS_TEMPLATES, 'Bewerber und Recruiting', ['simple']),
      )[0],
    ).toBe('hiring');
    expect(
      ids(rankTemplates(FIELD_TEMPLATES, 'Kundensupport für Kunden'))[0],
    ).toBe('support');
  });

  it('falls back when nothing fits, and returns nothing without a fallback', () => {
    expect(ids(rankTemplates(STATUS_TEMPLATES, 'zzz qqq', ['simple']))).toEqual(
      ['simple'],
    );
    expect(rankTemplates(FIELD_TEMPLATES, 'zzz qqq')).toEqual([]);
  });

  it('only matches whole words', () => {
    expect(rankTemplates(FIELD_TEMPLATES, 'application', [])).toEqual([]);
  });
});

describe('the library itself', () => {
  it('ends every workflow in one completed status and names everything in both languages', () => {
    for (const t of STATUS_TEMPLATES) {
      expect(t.stages.at(-1)?.done).toBe(true);
      expect(t.stages.filter((s) => s.done)).toHaveLength(1);
      expect(t.stages.every((s) => s.name.de && s.name.en)).toBe(true);
    }
    const fields = FIELD_TEMPLATES.flatMap((t) => t.fields);
    expect(fields.every((f) => f.name.de && f.name.en)).toBe(true);
    const dropdowns = fields.filter((f) => f.type === 'dropdown');
    expect(dropdowns.length).toBeGreaterThan(5);
    expect(dropdowns.every((f) => (f.items?.length ?? 0) > 1)).toBe(true);
  });

  it('picks the wording by locale', () => {
    expect(label({ en: 'Done', de: 'Erledigt' }, 'de')).toBe('Erledigt');
    expect(label({ en: 'Done', de: 'Erledigt' }, 'en')).toBe('Done');
  });
});
