'use client';

import { Button } from '@heroui/react';
import { Plus, Sparkles, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslations } from 'next-intl';
import type { FieldOut, StageOut } from '../../api/generated/model';
import { fullName } from '../../lib/format';
import {
  parseFilterText,
  TYPE_FIELD,
  type FilterRow,
} from '../../lib/filter-text';
import { itemsOf } from '../cards/field-input';
import { SimpleSelect, TextInput } from '../ui/inputs';
import type { Member } from './types';
import { useAiStatus } from '../../lib/ai';

export type { FilterRow };

const DATE_FIELDS = ['created_at', 'updated_at'];
const DATE_OPS = ['gte', 'lte', 'gt', 'lt'];

/** An RFC 3339 instant for a day picked in the builder (the server wants one for `created_at`). */
const startOfDay = (v: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(v) ? `${v}T00:00:00Z` : v;

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

/** What a row is about: the status, the issue type, one of the two dates, or one of the issue's fields. */
function rowKind(slug: string, f: FieldOut | undefined) {
  if (slug === 'stage' || slug === TYPE_FIELD) return 'stage' as const;
  if (DATE_FIELDS.includes(slug)) return 'timestamp' as const;
  return kind(f);
}

/** The field name the server knows for a row. */
function targetOf(slug: string) {
  if (slug === 'stage') return 'stage';
  if (slug === TYPE_FIELD) return 'card_type_id';
  return DATE_FIELDS.includes(slug) ? slug : `data.${slug}`;
}

export function rowsToFilter(
  rows: FilterRow[],
  fields: FieldOut[],
): { and: unknown[] } | null {
  const and = rows
    .filter((r) => r.field && r.operator)
    .map((r) => {
      const f = fields.find((x) => x.slug === r.field);
      const k = rowKind(r.field, f);
      const target = targetOf(r.field);
      if (r.operator === 'isNull' || r.operator === 'isNotNull')
        return { field: target, operator: r.operator };
      let value: unknown = k === 'timestamp' ? startOfDay(r.value) : r.value;
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
  return and.map((c) => {
    const raw = String(c.field ?? '').replace(/^data\./, '');
    const field = raw === 'card_type_id' ? TYPE_FIELD : raw;
    const value = Array.isArray(c.value)
      ? String(c.value[0] ?? '')
      : c.value === undefined
        ? ''
        : String(c.value);
    return {
      field,
      operator: String(c.operator ?? ''),
      value: DATE_FIELDS.includes(field) ? value.slice(0, 10) : value,
    };
  });
}

export function FilterBuilder({
  rows,
  onChange,
  fields,
  stages,
  members,
  cardTypes = [],
  meId,
}: {
  rows: FilterRow[];
  onChange: (rows: FilterRow[]) => void;
  fields: FieldOut[];
  stages: StageOut[];
  members: Member[];
  cardTypes?: { id: string; name: string; icon?: string }[];
  /** The signed-in person, for "assigned to me" in a described filter. */
  meId?: string;
}) {
  const t = useTranslations('views');
  const ai = useAiStatus();
  const [sentence, setSentence] = useState('');
  const [note, setNote] = useState<string | null>(null);
  const describe = () => {
    const parsed = parseFilterText(sentence, {
      fields,
      stages,
      members,
      cardTypes,
      meId,
    });
    if (parsed.rows.length === 0) return setNote(t('describeNone'));
    setNote(null);
    onChange(parsed.rows);
    setSentence('');
  };
  const fieldOptions = [
    { value: 'stage', label: t('stage') },
    ...(cardTypes.length > 0
      ? [{ value: TYPE_FIELD, label: t('issueType') }]
      : []),
    { value: 'created_at', label: t('createdAt') },
    { value: 'updated_at', label: t('updatedAt') },
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
      {ai && (
        <form
          className="flex items-end gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            describe();
          }}
        >
          <div className="min-w-0 flex-1">
            <TextInput
              label={t('describeFilter')}
              placeholder={t('describePlaceholder')}
              value={sentence}
              onChange={setSentence}
            />
          </div>
          <Button
            type="submit"
            size="sm"
            variant="secondary"
            isDisabled={!sentence.trim()}
            data-testid="describe-filter"
          >
            <Sparkles size={14} />
            {t('describeApply')}
          </Button>
        </form>
      )}
      {ai && note && <p className="text-sm text-muted">{note}</p>}
      {rows.length === 0 && (
        <p className="text-sm text-muted">{t('noFilters')}</p>
      )}
      {rows.map((r, i) => {
        const f = fields.find((x) => x.slug === r.field);
        const k = rowKind(r.field, f);
        const ops =
          k === 'timestamp'
            ? DATE_OPS
            : k === 'number' || k === 'date'
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
                options={
                  r.field === TYPE_FIELD
                    ? cardTypes.map((c) => ({ value: c.id, label: c.name }))
                    : stages.map((s) => ({ value: s.id, label: s.name }))
                }
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
                  k === 'number'
                    ? 'number'
                    : k === 'date' || k === 'timestamp'
                      ? 'date'
                      : 'text'
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
