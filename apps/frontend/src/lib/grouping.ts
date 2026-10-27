import type { CardOut, FieldOut, StageOut } from '../api/generated/model';
import { asStrings, stageOf } from './cards';

export type GroupBy = {
  type: 'list_stage' | 'field' | 'all';
  fieldId?: string;
};
export type Group = { key: string; label: string; cards: CardOut[] };

/** Field types that make sense to group by: they hold a small set of values. */
export const GROUPABLE = ['dropdown', 'label', 'user', 'checkbox'];

const NONE = '__none';

/**
 * Splits `cards` into groups, in a stable order:
 * - by stage: one group per stage of the list in board order, empty ones included, so the
 *   workflow is visible even where nothing sits;
 * - by field: one group per value, in the field's own order (dropdown/label items), then the
 *   rest alphabetically, and "no value" last. A card with several values (labels) appears in each
 *   of its groups.
 * Returns `null` when there is nothing to group by, so callers render the plain list.
 */
export function groupCards(
  cards: CardOut[],
  groupBy: GroupBy | undefined,
  ctx: {
    listId: string;
    stages: StageOut[];
    fields: FieldOut[];
    members: { user_id: string; first_name: string; last_name: string }[];
    noValue: string;
  },
): Group[] | null {
  if (!groupBy || groupBy.type === 'all') return null;

  if (groupBy.type === 'list_stage') {
    const groups = ctx.stages.map((s) => ({
      key: s.id,
      label: s.name,
      cards: [] as CardOut[],
    }));
    const orphans: CardOut[] = [];
    for (const card of cards) {
      const g = groups.find((x) => x.key === stageOf(card, ctx.listId));
      (g?.cards ?? orphans).push(card);
    }
    return orphans.length
      ? [...groups, { key: NONE, label: ctx.noValue, cards: orphans }]
      : groups;
  }

  const field = ctx.fields.find((f) => f.id === groupBy.fieldId);
  if (!field) return null;
  const label = (value: string) => {
    if (field.type === 'user') {
      const m = ctx.members.find((x) => x.user_id === value);
      return m ? `${m.first_name} ${m.last_name}`.trim() : value;
    }
    return value;
  };
  const byValue = new Map<string, CardOut[]>();
  const without: CardOut[] = [];
  for (const card of cards) {
    const raw = card.data[field.slug];
    const values =
      field.type === 'checkbox'
        ? raw === true
          ? ['true']
          : raw === false
            ? ['false']
            : []
        : asStrings(raw);
    if (values.length === 0) without.push(card);
    for (const v of new Set(values))
      byValue.set(v, [...(byValue.get(v) ?? []), card]);
  }
  const declared = Array.isArray(field.items)
    ? (field.items as unknown[]).filter(
        (i): i is string => typeof i === 'string',
      )
    : [];
  const order = (v: string) =>
    declared.includes(v) ? declared.indexOf(v) : declared.length;
  const groups = [...byValue.entries()]
    .sort(([a], [b]) => order(a) - order(b) || label(a).localeCompare(label(b)))
    .map(([value, cs]) => ({ key: value, label: label(value), cards: cs }));
  return without.length
    ? [...groups, { key: NONE, label: ctx.noValue, cards: without }]
    : groups;
}
