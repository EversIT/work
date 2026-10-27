'use client';

import { Button } from '@heroui/react';
import { Plus, Trash2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import type { FieldOut, StageOut } from '../../api/generated/model';
import { fullName } from '../../lib/format';
import { itemsOf } from '../cards/field-input';
import { SimpleSelect, TextInput } from '../ui/inputs';
import type { Member } from './types';

export type FilterRow = { field: string; operator: string; value: string };

const TEXT_OPS = ['like', 'eq', 'ne', 'isNull', 'isNotNull'];
const NUM_OPS = ['eq', 'ne', 'gt', 'gte', 'lt', 'lte', 'isNull', 'isNotNull'];
const PICK_OPS = ['in', 'nin', 'isNull', 'isNotNull'];

function kind(
  field: FieldOut | undefined,
): 'text' | 'number' | 'date' | 'bool' | 'pick' | 'stage' {
  if (!field) return 'stage';
  switch (field.type) {
    case 'number':
    case 'percentage':
    case 'currency':
      return 'number';
    case 'date':
    case 'date_time':
      return 'date';
    case 'checkbox':
      return 'bool';
    case 'dropdown':
    case 'label':
    case 'user':
      return 'pick';
    default:
      return 'text';
  }
}

export function rowsToFilter(
  rows: FilterRow[],
  fields: FieldOut[],
): { and: unknown[] } | null {
  const and = rows
    .filter((r) => r.field && r.operator)
    .map((r) => {
      const f = fields.find((x) => x.slug === r.field);
      const k = r.field === 'stage' ? 'stage' : kind(f);
      const target = r.field === 'stage' ? 'stage' : `data.${r.field}`;
      if (r.operator === 'isNull' || r.operator === 'isNotNull')
        return { field: target, operator: r.operator };
      let value: unknown = r.value;
      if (k === 'number') value = Number(r.value);
      if (k === 'bool') value = r.value === 'true';
      if (k === 'pick' || k === 'stage') value = [r.value];
      return { field: target, operator: r.operator, value };
    })
    .filter(
      (c) =>
        c.operator === 'isNull' ||
        c.operator === 'isNotNull' ||
        (c as { value?: unknown }).value !== '',
    );
  return and.length ? { and } : null;
}

export function filterToRows(filters: unknown): FilterRow[] {
  const and = (
    filters as {
      and?: { field?: string; operator?: string; value?: unknown }[];
    } | null
  )?.and;
  if (!Array.isArray(and)) return [];
  return and.map((c) => ({
    field: String(c.field ?? '').replace(/^data\./, ''),
    operator: String(c.operator ?? ''),
    value: Array.isArray(c.value)
      ? String(c.value[0] ?? '')
      : c.value === undefined
        ? ''
        : String(c.value),
  }));
}

export function FilterBuilder({
  rows,
  onChange,
  fields,
  stages,
  members,
}: {
  rows: FilterRow[];
  onChange: (rows: FilterRow[]) => void;
  fields: FieldOut[];
  stages: StageOut[];
  members: Member[];
}) {
  const t = useTranslations('views');
  const fieldOptions = [
    { value: 'stage', label: t('stage') },
    ...fields
      .filter(
        (f) => f.type !== 'rich' && f.type !== 'card' && f.type !== 'stage',
      )
      .map((f) => ({ value: f.slug, label: f.name })),
  ];
  const set = (i: number, patch: Partial<FilterRow>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  return (
    <div
      className="flex w-[min(92vw,36rem)] flex-col gap-3 p-3"
      data-testid="filter-builder"
    >
      {rows.length === 0 && (
        <p className="text-sm text-muted">{t('noFilters')}</p>
      )}
      {rows.map((r, i) => {
        const f = fields.find((x) => x.slug === r.field);
        const k = r.field === 'stage' ? 'stage' : kind(f);
        const ops =
          k === 'number' || k === 'date'
            ? NUM_OPS
            : k === 'pick' || k === 'stage'
              ? PICK_OPS
              : k === 'bool'
                ? ['eq']
                : TEXT_OPS;
        const needsValue =
          r.operator !== 'isNull' && r.operator !== 'isNotNull';
        let valueInput: React.ReactNode = null;
        if (needsValue) {
          if (k === 'stage')
            valueInput = (
              <SimpleSelect
                label={t('value')}
                value={r.value || null}
                onChange={(v) => set(i, { value: v ?? '' })}
                options={stages.map((s) => ({ value: s.id, label: s.name }))}
              />
            );
          else if (k === 'pick' && f?.type === 'user')
            valueInput = (
              <SimpleSelect
                label={t('value')}
                value={r.value || null}
                onChange={(v) => set(i, { value: v ?? '' })}
                options={members.map((m) => ({
                  value: m.user_id,
                  label: fullName(m),
                }))}
              />
            );
          else if (k === 'pick' && f)
            valueInput = (
              <SimpleSelect
                label={t('value')}
                value={r.value || null}
                onChange={(v) => set(i, { value: v ?? '' })}
                options={itemsOf(f).map((x) => ({ value: x, label: x }))}
              />
            );
          else if (k === 'bool')
            valueInput = (
              <SimpleSelect
                label={t('value')}
                value={r.value || 'true'}
                onChange={(v) => set(i, { value: v ?? 'true' })}
                options={[
                  { value: 'true', label: '✓' },
                  { value: 'false', label: '✗' },
                ]}
              />
            );
          else
            valueInput = (
              <TextInput
                label={t('value')}
                type={
                  k === 'number' ? 'number' : k === 'date' ? 'date' : 'text'
                }
                value={r.value}
                onChange={(v) => set(i, { value: v })}
              />
            );
        }
        return (
          <div
            key={i}
            className="grid grid-cols-[1fr_1fr_1fr_auto] items-end gap-2"
          >
            <SimpleSelect
              label={t('field')}
              value={r.field || null}
              onChange={(v) =>
                set(i, { field: v ?? '', operator: '', value: '' })
              }
              options={fieldOptions}
            />
            <SimpleSelect
              label={t('operator')}
              value={r.operator || null}
              onChange={(v) => set(i, { operator: v ?? '' })}
              options={ops.map((o) => ({ value: o, label: t(`op.${o}`) }))}
            />
            <div>{valueInput}</div>
            <Button
              isIconOnly
              variant="ghost"
              aria-label={t('removeFilter')}
              onPress={() => onChange(rows.filter((_, j) => j !== i))}
            >
              <Trash2 size={14} />
            </Button>
          </div>
        );
      })}
      <Button
        variant="tertiary"
        size="sm"
        className="self-start"
        onPress={() =>
          onChange([...rows, { field: '', operator: '', value: '' }])
        }
        data-testid="add-filter"
      >
        <Plus size={14} />
        {t('addFilter')}
      </Button>
    </div>
  );
}
