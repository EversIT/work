'use client';

import { Chip } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useEffect, useState } from 'react';
import { explainFailure, summarizeRuns } from '../../lib/automation-text';
import { useListAutomationRuns } from '../../api/generated/automations/automations';
import { useGetList } from '../../api/generated/lists/lists';
import type { FieldOut, StepIo } from '../../api/generated/model';
import { body } from '../../lib/api';
import { fullName, relativeTime } from '../../lib/format';
import { itemsOf } from '../cards/field-input';
import { SimpleSelect, TextAreaInput, TextInput } from '../ui/inputs';
import { useAiStatus } from '../../lib/ai';

export const TRIGGERS = [
  'card_created',
  'field_updated',
  'stage_changed',
  'comment_created',
  'assignee_added',
  'assignee_removed',
  'date_arrives',
  'webhook',
] as const;
export const ACTIONS = [
  'set_field',
  'set_stage',
  'change_assignee',
  'create_comment',
  'create_card',
  'create_sub_card',
  'send_webhook',
  'send_email',
] as const;

/** A rule as the builder edits it. */
export type Draft = {
  name: string;
  trigger_type: string;
  trigger_config: Record<string, unknown>;
  steps: StepIo[];
  /** The project the rule is limited to; `null` = every project of the workspace. */
  list: string | null;
};

export function Runs({ id, locale }: { id: string; locale: string }) {
  const t = useTranslations('automations');
  const runs = body(useListAutomationRuns(id).data) ?? [];
  const ai = useAiStatus();
  const [summary, setSummary] = useState<string | null>(null);
  if (runs.length === 0)
    return <p className="text-sm text-muted">{t('noRuns')}</p>;
  return (
    <>
      <div className="mb-2 flex flex-col gap-1">
        {ai && (
          <button
            type="button"
            onClick={() => setSummary(summarizeRuns(runs, t))}
            className="self-start text-sm text-accent hover:underline"
            data-testid="summarize-runs"
          >
            {t('summarizeFailures')}
          </button>
        )}
        {summary && (
          <p className="text-sm" data-testid="runs-summary">
            {summary}
          </p>
        )}
      </div>
      <ul className="divide-y divide-separator text-sm">
        {runs.map((r) => (
          <li
            key={r.id}
            className="flex flex-wrap items-center justify-between gap-x-3 py-1"
          >
            <span>{relativeTime(r.started_at, locale)}</span>
            <Chip
              size="sm"
              color={
                r.status === 'completed'
                  ? 'success'
                  : r.status === 'failed'
                    ? 'danger'
                    : 'default'
              }
            >
              <Chip.Label>{r.status}</Chip.Label>
            </Chip>
            {r.error && <span className="text-xs text-danger">{r.error}</span>}
            {ai && r.error && explainFailure(r.error, t) && (
              <span
                className="w-full text-xs text-muted"
                data-testid="why-failed"
              >
                {t('whyFailed')} {explainFailure(r.error, t)}
              </span>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}

export type Member = { user_id: string; first_name: string; last_name: string };

export function TriggerConfig({
  draft,
  fields,
  members,
  listId,
  onChange,
}: {
  draft: Draft;
  fields: FieldOut[];
  members: Member[];
  listId?: string;
  onChange: (c: Record<string, unknown>) => void;
}) {
  const t = useTranslations('automations');
  const list = body(
    useGetList(listId ?? '', {
      query: { enabled: !!listId && draft.trigger_type === 'stage_changed' },
    }).data,
  );
  const c = draft.trigger_config;
  const set = (k: string, v: unknown) =>
    onChange({
      ...c,
      [k]: v === null || v === '' || v === '__none' ? undefined : v,
    });
  if (draft.trigger_type === 'field_updated') {
    const f = fields.find((x) => x.slug === c.field);
    const opts = f ? itemsOf(f) : [];
    return (
      <div className="grid gap-3">
        <SimpleSelect
          label={t('field')}
          value={(c.field as string) ?? null}
          allowEmpty
          onChange={(v) => onChange({ field: v === '__none' ? undefined : v })}
          options={fields
            .filter((x) => !['rich', 'card'].includes(x.type))
            .map((x) => ({ value: x.slug, label: x.name }))}
        />
        {opts.length > 0 ? (
          <>
            <SimpleSelect
              label={t('from')}
              value={(c.from as string) ?? null}
              allowEmpty
              onChange={(v) => set('from', v)}
              options={opts.map((o) => ({ value: o, label: o }))}
            />
            <SimpleSelect
              label={t('to')}
              value={(c.to as string) ?? null}
              allowEmpty
              onChange={(v) => set('to', v)}
              options={opts.map((o) => ({ value: o, label: o }))}
            />
          </>
        ) : (
          <>
            <TextInput
              label={t('from')}
              value={(c.from as string) ?? ''}
              onChange={(v) => set('from', v)}
            />
            <TextInput
              label={t('to')}
              value={(c.to as string) ?? ''}
              onChange={(v) => set('to', v)}
            />
          </>
        )}
      </div>
    );
  }
  if (draft.trigger_type === 'stage_changed') {
    const stages = list?.stages ?? [];
    return (
      <div className="grid gap-3">
        <SimpleSelect
          label={t('from')}
          value={((c.from as string[]) ?? [])[0] ?? null}
          allowEmpty
          onChange={(v) => set('from', v && v !== '__none' ? [v] : undefined)}
          options={stages.map((s) => ({ value: s.id, label: s.name }))}
        />
        <SimpleSelect
          label={t('to')}
          value={((c.to as string[]) ?? [])[0] ?? null}
          allowEmpty
          onChange={(v) => set('to', v && v !== '__none' ? [v] : undefined)}
          options={stages.map((s) => ({ value: s.id, label: s.name }))}
          testId="trigger-to"
        />
      </div>
    );
  }
  if (draft.trigger_type === 'date_arrives') {
    const dates = fields.filter(
      (x) => x.type === 'date' || x.type === 'date_time',
    );
    // Hours before (negative) or after (positive) the date; stored as minutes.
    const hours =
      typeof c.offset_minutes === 'number' ? String(c.offset_minutes / 60) : '';
    return (
      <div className="grid gap-3">
        <SimpleSelect
          label={t('dateField')}
          value={(c.field as string) ?? 'due_at'}
          onChange={(v) => v && onChange({ ...c, field: v })}
          options={dates.map((x) => ({ value: x.slug, label: x.name }))}
          testId="date-field"
        />
        <TextInput
          label={t('offsetHours')}
          value={hours}
          onChange={(v) => {
            const n = Number(v);
            onChange({
              ...c,
              offset_minutes:
                v.trim() === '' || !Number.isFinite(n)
                  ? undefined
                  : Math.round(n * 60),
            });
          }}
          placeholder="-24"
        />
        <p className="text-xs text-muted">{t('dateHint')}</p>
      </div>
    );
  }
  if (
    draft.trigger_type === 'assignee_added' ||
    draft.trigger_type === 'assignee_removed'
  ) {
    return (
      <SimpleSelect
        label={t('user')}
        value={(c.user_id as string) ?? null}
        allowEmpty
        onChange={(v) => set('user_id', v)}
        options={members.map((m) => ({ value: m.user_id, label: fullName(m) }))}
      />
    );
  }
  return null;
}

export function StepConfig({
  step,
  fields,
  members,
  lists,
  defaultList,
  onChange,
}: {
  step: StepIo;
  fields: FieldOut[];
  members: Member[];
  lists: { id: string; name: string }[];
  defaultList?: string;
  onChange: (c: Record<string, unknown>) => void;
}) {
  const t = useTranslations('automations');
  const c = step.config as Record<string, unknown>;
  const targetList = (c.list_id as string) ?? defaultList;
  const list = body(
    useGetList(targetList ?? '', {
      query: {
        enabled:
          !!targetList &&
          ['set_stage', 'create_card'].includes(step.action_type),
      },
    }).data,
  );
  // What the form shows must be what gets saved: persist the default list right away.
  useEffect(() => {
    if (step.action_type === 'create_card' && !c.list_id && targetList)
      onChange({ ...c, list_id: targetList });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step.action_type, targetList]);
  switch (step.action_type) {
    case 'set_field': {
      const f = fields.find((x) => x.slug === c.field);
      const coerce = (v: string): unknown =>
        f && ['number', 'percentage', 'currency'].includes(f.type)
          ? v === ''
            ? null
            : Number(v)
          : f?.type === 'checkbox'
            ? v === 'true'
            : f && ['user', 'dropdown', 'label'].includes(f.type) && f.multiple
              ? [v]
              : v;
      const opts = f
        ? f.type === 'user'
          ? members.map((m) => ({ value: m.user_id, label: fullName(m) }))
          : itemsOf(f).map((o) => ({ value: o, label: o }))
        : [];
      return (
        <div className="grid gap-3">
          <SimpleSelect
            label={t('field')}
            value={(c.field as string) ?? null}
            onChange={(v) => v && onChange({ field: v })}
            options={fields
              .filter((x) => !['rich', 'card'].includes(x.type))
              .map((x) => ({ value: x.slug, label: x.name }))}
          />
          {opts.length > 0 ? (
            <SimpleSelect
              label={t('value')}
              value={
                Array.isArray(c.value)
                  ? (c.value[0] as string)
                  : ((c.value as string) ?? null)
              }
              onChange={(v) => v && onChange({ ...c, value: coerce(v) })}
              options={opts}
            />
          ) : (
            <TextInput
              label={t('value')}
              value={
                c.value === undefined || c.value === null ? '' : String(c.value)
              }
              onChange={(v) => onChange({ ...c, value: coerce(v) })}
            />
          )}
        </div>
      );
    }
    case 'set_stage':
      return (
        <div className="grid gap-3">
          <SimpleSelect
            label={t('list')}
            value={targetList ?? null}
            onChange={(v) => v && onChange({ list_id: v })}
            options={lists.map((l) => ({ value: l.id, label: l.name }))}
          />
          <SimpleSelect
            label={t('stage')}
            value={(c.stage_id as string) ?? null}
            onChange={(v) => v && onChange({ ...c, stage_id: v })}
            options={(list?.stages ?? []).map((s) => ({
              value: s.id,
              label: s.name,
            }))}
            testId="set-stage"
          />
        </div>
      );
    case 'change_assignee':
      return (
        <div className="grid gap-3">
          <SimpleSelect
            label={t('add')}
            value={((c.add as string[]) ?? [])[0] ?? null}
            allowEmpty
            onChange={(v) =>
              onChange({ ...c, add: v && v !== '__none' ? [v] : [] })
            }
            options={members.map((m) => ({
              value: m.user_id,
              label: fullName(m),
            }))}
          />
          <SimpleSelect
            label={t('remove')}
            value={((c.remove as string[]) ?? [])[0] ?? null}
            allowEmpty
            onChange={(v) =>
              onChange({ ...c, remove: v && v !== '__none' ? [v] : [] })
            }
            options={members.map((m) => ({
              value: m.user_id,
              label: fullName(m),
            }))}
          />
        </div>
      );
    case 'create_comment':
      return (
        <TextAreaInput
          label={t('comment')}
          rows={3}
          value={(c.content as string) ?? ''}
          onChange={(v) => onChange({ content: v })}
          maxLength={20000}
        />
      );
    case 'create_card':
      return (
        <div className="grid gap-3">
          <SimpleSelect
            label={t('list')}
            value={targetList ?? null}
            onChange={(v) => v && onChange({ ...c, list_id: v })}
            options={lists.map((l) => ({ value: l.id, label: l.name }))}
          />
          <SimpleSelect
            label={t('stage')}
            value={(c.stage_id as string) ?? null}
            allowEmpty
            onChange={(v) =>
              onChange({ ...c, stage_id: v && v !== '__none' ? v : undefined })
            }
            options={(list?.stages ?? []).map((s) => ({
              value: s.id,
              label: s.name,
            }))}
          />
          <TextInput
            label={t('cardTitle')}
            value={String(
              ((c.data as Record<string, unknown>) ?? {}).title ?? '',
            )}
            onChange={(v) => onChange({ ...c, data: { title: v } })}
            maxLength={500}
          />
        </div>
      );
    case 'send_webhook':
      return (
        <div className="flex flex-col gap-2">
          <TextInput
            label={t('url')}
            value={(c.url as string) ?? ''}
            onChange={(v) => onChange({ ...c, url: v })}
            placeholder="https://hooks.example.com/…"
            maxLength={2000}
          />
          <TextAreaInput
            label={t('payload')}
            rows={3}
            value={
              typeof c.body === 'string'
                ? c.body
                : c.body
                  ? JSON.stringify(c.body, null, 2)
                  : ''
            }
            onChange={(v) => {
              let parsed: unknown = v;
              try {
                parsed = v.trim() ? JSON.parse(v) : undefined;
              } catch {
                /* keep the raw text until it is valid JSON */
              }
              onChange({ ...c, body: parsed });
            }}
          />
          <p className="text-xs text-muted">
            {t('payloadHint', { example: '{{trigger.card.key}}' })}
          </p>
        </div>
      );
    case 'send_email':
      return (
        <div className="flex flex-col gap-2">
          <SimpleSelect
            label={t('to')}
            value={(c.to as string) ?? null}
            onChange={(v) => v && onChange({ ...c, to: v })}
            options={members.map((m) => ({
              value: m.user_id,
              label: fullName(m),
            }))}
            testId="email-to"
          />
          <TextInput
            label={t('subject')}
            value={(c.subject as string) ?? ''}
            onChange={(v) => onChange({ ...c, subject: v })}
            maxLength={200}
          />
          <TextAreaInput
            label={t('emailBody')}
            rows={4}
            value={(c.body as string) ?? ''}
            onChange={(v) => onChange({ ...c, body: v })}
            maxLength={20000}
          />
        </div>
      );
    case 'create_sub_card':
      return (
        <TextInput
          label={t('cardTitle')}
          value={String(
            ((c.data as Record<string, unknown>) ?? {}).title ?? '',
          )}
          onChange={(v) => onChange({ ...c, data: { title: v } })}
          maxLength={500}
        />
      );
    default:
      return null;
  }
}
