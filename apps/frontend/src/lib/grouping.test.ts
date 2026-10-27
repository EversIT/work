import { describe, expect, it } from 'vitest';
import type { CardOut, FieldOut, StageOut } from '../api/generated/model';
import { groupCards } from './grouping';

const card = (id: string, data: Record<string, unknown>, stage?: string) =>
  ({
    id,
    data,
    lists: stage ? [{ list_id: 'L', list_stage_id: stage }] : [],
  }) as unknown as CardOut;
const stages = [
  { id: 's1', name: 'To Do' },
  { id: 's2', name: 'Doing' },
  { id: 's3', name: 'Done' },
] as StageOut[];
const field = (type: string, extra: Partial<FieldOut> = {}) =>
  ({ id: 'f1', slug: 'x', type, ...extra }) as unknown as FieldOut;
const ctx = (fields: FieldOut[] = []) => ({
  listId: 'L',
  stages,
  fields,
  members: [{ user_id: 'u1', first_name: 'Ada', last_name: 'L' }],
  noValue: 'None',
});

describe('groupCards', () => {
  it('does not group without a setting', () => {
    expect(groupCards([card('a', {})], undefined, ctx())).toBeNull();
    expect(groupCards([card('a', {})], { type: 'all' }, ctx())).toBeNull();
  });

  it('groups by stage in board order and keeps empty stages', () => {
    const g = groupCards(
      [card('a', {}, 's2'), card('b', {}, 's1'), card('c', {}, 's1')],
      { type: 'list_stage' },
      ctx(),
    )!;
    expect(g.map((x) => [x.label, x.cards.map((c) => c.id)])).toEqual([
      ['To Do', ['b', 'c']],
      ['Doing', ['a']],
      ['Done', []],
    ]);
  });

  it('puts cards outside every stage into a last group', () => {
    const g = groupCards(
      [card('a', {}, 's1'), card('b', {})],
      { type: 'list_stage' },
      ctx(),
    )!;
    expect(g.at(-1)).toMatchObject({ label: 'None', cards: [{ id: 'b' }] });
  });

  it('groups by a dropdown in the order of its options, "no value" last', () => {
    const f = field('dropdown', {
      items: ['High', 'Low'] as unknown as FieldOut['items'],
    });
    const g = groupCards(
      [
        card('a', { x: 'Low' }),
        card('b', {}),
        card('c', { x: 'High' }),
        card('d', { x: 'Other' }),
      ],
      { type: 'field', fieldId: 'f1' },
      ctx([f]),
    )!;
    expect(g.map((x) => x.label)).toEqual(['High', 'Low', 'Other', 'None']);
  });

  it('lists a card with several labels under each of them', () => {
    const g = groupCards(
      [card('a', { x: ['red', 'blue'] })],
      { type: 'field', fieldId: 'f1' },
      ctx([field('label')]),
    )!;
    expect(g.map((x) => x.label).sort()).toEqual(['blue', 'red']);
    expect(g.every((x) => x.cards.length === 1)).toBe(true);
  });

  it('shows people by name and handles checkboxes', () => {
    const people = groupCards(
      [card('a', { x: 'u1' })],
      { type: 'field', fieldId: 'f1' },
      ctx([field('user')]),
    )!;
    expect(people[0].label).toBe('Ada L');
    const flags = groupCards(
      [card('a', { x: true }), card('b', { x: false }), card('c', {})],
      { type: 'field', fieldId: 'f1' },
      ctx([field('checkbox')]),
    )!;
    expect(flags.map((x) => x.label)).toEqual(['false', 'true', 'None']);
  });

  it('does not group by a field that no longer exists', () => {
    expect(
      groupCards([card('a', {})], { type: 'field', fieldId: 'gone' }, ctx()),
    ).toBeNull();
  });
});
