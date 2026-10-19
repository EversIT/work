'use client';

import { Button, Card, Chip, Switch, toast, Label } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Plus, Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useEffect, useState } from 'react';
import { getSigningSecret, regenerateWebhookToken, useCreateAutomation, useDeleteAutomation, useListAutomationRuns, useListAutomations, useUpdateAutomation } from '../../api/generated/automations/automations';
import { Copy } from 'lucide-react';
import { useGetList } from '../../api/generated/lists/lists';
import type { AutomationOut, FieldOut, StepIo } from '../../api/generated/model';
import { useGetWorkspaceTree } from '../../api/generated/spaces/spaces';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName, relativeTime } from '../../lib/format';
import { useCardTypes, useMembers, useWorkspace } from '../../lib/hooks';
import { itemsOf } from '../cards/field-input';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextAreaInput, TextInput } from '../ui/inputs';

const TRIGGERS = ['card_created', 'field_updated', 'stage_changed', 'comment_created', 'assignee_added', 'assignee_removed', 'webhook'];
const ACTIONS = ['set_field', 'set_stage', 'change_assignee', 'create_comment', 'create_card', 'create_sub_card', 'send_webhook', 'send_email'];

type Draft = { name: string; trigger_type: string; trigger_config: Record<string, unknown>; steps: StepIo[]; list: string | null };

export function AutomationSettings({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('automations');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const workspace = useWorkspace(workspaceId);
  const members = useMembers(workspace?.project_id);
  const types = useCardTypes(workspaceId);
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  const items = body(useListAutomations(workspaceId).data) ?? [];
  const create = useCreateAutomation();
  const update = useUpdateAutomation();
  const del = useDeleteAutomation();
  const [editing, setEditing] = useState<{ id?: string; draft: Draft } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [runsFor, setRunsFor] = useState<string | null>(null);
  const [secret, setSecret] = useState<{ title: string; value: string; hint: string } | null>(null);
  const lists = tree?.spaces.flatMap((s) => s.lists) ?? [];
  const fields: FieldOut[] = types.flatMap((c) => c.fields).filter((f, i, a) => a.findIndex((x) => x.slug === f.slug) === i);
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const blank: Draft = { name: '', trigger_type: 'card_created', trigger_config: {}, steps: [{ action_type: 'create_comment', config: { content: '' } }], list: null };

  const fromAutomation = (a: AutomationOut): Draft => ({
    name: a.name,
    trigger_type: a.trigger_type,
    trigger_config: (a.trigger_config ?? {}) as Record<string, unknown>,
    steps: a.steps,
    list: a.locations.find((l) => l.location_type === 'list')?.location_id ?? null,
  });

  const save = async () => {
    if (!editing) return;
    setError(null);
    const d = editing.draft;
    const payload = {
      name: d.name,
      trigger_type: d.trigger_type,
      trigger_config: d.trigger_config,
      steps: d.steps,
      locations: d.list ? [{ location_type: 'list', location_id: d.list }] : [],
    };
    try {
      if (editing.id) await update.mutateAsync({ id: editing.id, data: payload });
      else await create.mutateAsync({ id: workspaceId, data: { ...payload, is_enabled: true } });
      await refresh();
      setEditing(null);
    } catch (e) {
      setError(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-medium">{t('title')}</h2>
        <Button size="sm" onPress={() => { setError(null); setEditing({ draft: blank }); }} data-testid="new-automation"><Plus size={14} />{t('new')}</Button>
      </div>
      <p className="text-sm text-muted">{t('adminOnly')}</p>
      {items.length === 0 && <p className="text-muted">{t('empty')}</p>}
      {items.map((a) => (
        <Card key={a.id} data-testid={`automation-${a.name}`}>
          <Card.Header className="flex-row items-center justify-between">
            <div>
              <Card.Title>{a.name}</Card.Title>
              <Card.Description>{t(`trigger.${a.trigger_type}`)} → {a.steps.map((s) => t(`action.${s.action_type}`)).join(', ')}</Card.Description>
            </div>
            <div className="flex items-center gap-2">
              <Switch className="flex flex-row items-center gap-2" isSelected={a.is_enabled} onChange={async (v) => { try { await update.mutateAsync({ id: a.id, data: { is_enabled: v } }); await refresh(); } catch (e) { fail(e); } }}>
                <Switch.Control><Switch.Thumb /></Switch.Control>
                <Switch.Content><Label>{t('enabled')}</Label></Switch.Content>
              </Switch>
              {a.trigger_type === 'webhook' && (
                <Button size="sm" variant="tertiary" data-testid={`webhook-url-${a.name}`} onPress={async () => {
                  if (a.has_webhook && !window.confirm(t('confirmRotate'))) return;
                  try {
                    const r = await regenerateWebhookToken(a.id);
                    if (r.status === 200) setSecret({ title: t('webhookUrl'), value: `${window.location.origin}${r.data.path}`, hint: t('webhookUrlHint') });
                    await refresh();
                  } catch (e) { fail(e); }
                }}>{a.has_webhook ? t('rotateUrl') : t('createUrl')}</Button>
              )}
              {a.steps.some((st) => st.action_type === 'send_webhook') && (
                <Button size="sm" variant="tertiary" data-testid={`signing-secret-${a.name}`} onPress={async () => {
                  try {
                    const r = await getSigningSecret(a.id);
                    if (r.status === 200) setSecret({ title: t('signingSecret'), value: r.data.secret, hint: t('signingSecretHint') });
                  } catch (e) { fail(e); }
                }}>{t('signingSecret')}</Button>
              )}
              <Button size="sm" variant="tertiary" onPress={() => setRunsFor(runsFor === a.id ? null : a.id)}>{t('runs')}</Button>
              <Button size="sm" variant="tertiary" onPress={() => { setError(null); setEditing({ id: a.id, draft: fromAutomation(a) }); }}>{t('edit')}</Button>
              <Button isIconOnly size="sm" variant="ghost" aria-label={t('delete')} onPress={async () => { if (!window.confirm(t('confirmDelete'))) return; try { await del.mutateAsync({ id: a.id }); await refresh(); } catch (e) { fail(e); } }}><Trash2 size={14} /></Button>
            </div>
          </Card.Header>
          {runsFor === a.id && <Card.Content><Runs id={a.id} locale={locale} /></Card.Content>}
        </Card>
      ))}

      {secret && (
        <FormDialog isOpen onClose={() => setSecret(null)} title={secret.title} submitLabel={t('done')} onSubmit={() => setSecret(null)}>
          <p className="text-sm">{secret.hint}</p>
          <div className="flex items-center gap-2">
            <input readOnly aria-label={secret.title} value={secret.value} data-testid="secret-value" onFocus={(e) => e.currentTarget.select()} className="h-9 flex-1 rounded-[3px] border border-[#8590A2] bg-white px-2 font-mono text-xs" />
            <Button isIconOnly variant="tertiary" aria-label={t('copy')} onPress={() => navigator.clipboard?.writeText(secret.value).then(() => toast.success(t('copied')))}><Copy size={14} /></Button>
          </div>
        </FormDialog>
      )}

      {editing && (
        <FormDialog isOpen onClose={() => setEditing(null)} title={editing.id ? t('edit') : t('new')} size="lg" error={error} submitLabel={t('save')} isPending={create.isPending || update.isPending} onSubmit={save}>
          <TextInput label={t('name')} value={editing.draft.name} onChange={(v) => setEditing({ ...editing, draft: { ...editing.draft, name: v } })} required maxLength={100} />
          <SimpleSelect label={t('where')} value={editing.draft.list} allowEmpty onChange={(v) => setEditing({ ...editing, draft: { ...editing.draft, list: v === '__none' ? null : v } })} options={lists.map((l) => ({ value: l.id, label: l.name }))} placeholder={t('allLists')} />
          <h3 className="text-sm font-medium">{t('when')}</h3>
          <SimpleSelect label={t('triggerLabel')} value={editing.draft.trigger_type} onChange={(v) => v && setEditing({ ...editing, draft: { ...editing.draft, trigger_type: v, trigger_config: {} } })} options={TRIGGERS.map((x) => ({ value: x, label: t(`trigger.${x}`) }))} testId="trigger-type" />
          <TriggerConfig draft={editing.draft} fields={fields} members={members} listId={editing.draft.list ?? lists[0]?.id} onChange={(cfg) => setEditing({ ...editing, draft: { ...editing.draft, trigger_config: cfg } })} />
          <h3 className="text-sm font-medium">{t('then')}</h3>
          {editing.draft.steps.map((s, i) => (
            <div key={i} className="flex flex-col gap-2 rounded-lg border border-separator p-3">
              <div className="flex items-end gap-2">
                <div className="flex-1"><SimpleSelect label={t('actionLabel')} value={s.action_type} onChange={(v) => v && setEditing({ ...editing, draft: { ...editing.draft, steps: editing.draft.steps.map((x, j) => (j === i ? { action_type: v, config: {} } : x)) } })} options={ACTIONS.map((x) => ({ value: x, label: t(`action.${x}`) }))} testId={`action-type-${i}`} /></div>
                <Button isIconOnly variant="ghost" aria-label={t('removeStep')} isDisabled={editing.draft.steps.length <= 1} onPress={() => setEditing({ ...editing, draft: { ...editing.draft, steps: editing.draft.steps.filter((_, j) => j !== i) } })}><Trash2 size={14} /></Button>
              </div>
              <StepConfig step={s} fields={fields} members={members} lists={lists} defaultList={editing.draft.list ?? lists[0]?.id} onChange={(config) => setEditing({ ...editing, draft: { ...editing.draft, steps: editing.draft.steps.map((x, j) => (j === i ? { ...x, config } : x)) } })} />
            </div>
          ))}
          <Button variant="tertiary" size="sm" className="self-start" onPress={() => setEditing({ ...editing, draft: { ...editing.draft, steps: [...editing.draft.steps, { action_type: 'create_comment', config: { content: '' } }] } })}><Plus size={14} />{t('addStep')}</Button>
          <p className="text-xs text-muted">{t('placeholderHint', { example: '{{trigger.card.data.title}}' })}</p>
        </FormDialog>
      )}
    </div>
  );
}

function Runs({ id, locale }: { id: string; locale: string }) {
  const t = useTranslations('automations');
  const runs = body(useListAutomationRuns(id).data) ?? [];
  if (runs.length === 0) return <p className="text-sm text-muted">{t('noRuns')}</p>;
  return (
    <ul className="divide-y divide-separator text-sm">
      {runs.map((r) => (
        <li key={r.id} className="flex items-center justify-between py-1">
          <span>{relativeTime(r.started_at, locale)}</span>
          <Chip size="sm" color={r.status === 'completed' ? 'success' : r.status === 'failed' ? 'danger' : 'default'}><Chip.Label>{r.status}</Chip.Label></Chip>
          {r.error && <span className="text-xs text-danger">{r.error}</span>}
        </li>
      ))}
    </ul>
  );
}

type Member = { user_id: string; first_name: string; last_name: string };

function TriggerConfig({ draft, fields, members, listId, onChange }: { draft: Draft; fields: FieldOut[]; members: Member[]; listId?: string; onChange: (c: Record<string, unknown>) => void }) {
  const t = useTranslations('automations');
  const list = body(useGetList(listId ?? '', { query: { enabled: !!listId && draft.trigger_type === 'stage_changed' } }).data);
  const c = draft.trigger_config;
  const set = (k: string, v: unknown) => onChange({ ...c, [k]: v === null || v === '' || v === '__none' ? undefined : v });
  if (draft.trigger_type === 'field_updated') {
    const f = fields.find((x) => x.slug === c.field);
    const opts = f ? itemsOf(f) : [];
    return (
      <div className="grid gap-2 sm:grid-cols-3">
        <SimpleSelect label={t('field')} value={(c.field as string) ?? null} allowEmpty onChange={(v) => onChange({ field: v === '__none' ? undefined : v })} options={fields.filter((x) => !['rich', 'card'].includes(x.type)).map((x) => ({ value: x.slug, label: x.name }))} />
        {opts.length > 0 ? (
          <>
            <SimpleSelect label={t('from')} value={(c.from as string) ?? null} allowEmpty onChange={(v) => set('from', v)} options={opts.map((o) => ({ value: o, label: o }))} />
            <SimpleSelect label={t('to')} value={(c.to as string) ?? null} allowEmpty onChange={(v) => set('to', v)} options={opts.map((o) => ({ value: o, label: o }))} />
          </>
        ) : (
          <>
            <TextInput label={t('from')} value={(c.from as string) ?? ''} onChange={(v) => set('from', v)} />
            <TextInput label={t('to')} value={(c.to as string) ?? ''} onChange={(v) => set('to', v)} />
          </>
        )}
      </div>
    );
  }
  if (draft.trigger_type === 'stage_changed') {
    const stages = list?.stages ?? [];
    return (
      <div className="grid gap-2 sm:grid-cols-2">
        <SimpleSelect label={t('from')} value={((c.from as string[]) ?? [])[0] ?? null} allowEmpty onChange={(v) => set('from', v && v !== '__none' ? [v] : undefined)} options={stages.map((s) => ({ value: s.id, label: s.name }))} />
        <SimpleSelect label={t('to')} value={((c.to as string[]) ?? [])[0] ?? null} allowEmpty onChange={(v) => set('to', v && v !== '__none' ? [v] : undefined)} options={stages.map((s) => ({ value: s.id, label: s.name }))} testId="trigger-to" />
      </div>
    );
  }
  if (draft.trigger_type === 'assignee_added' || draft.trigger_type === 'assignee_removed') {
    return <SimpleSelect label={t('user')} value={(c.user_id as string) ?? null} allowEmpty onChange={(v) => set('user_id', v)} options={members.map((m) => ({ value: m.user_id, label: fullName(m) }))} />;
  }
  return null;
}

function StepConfig({ step, fields, members, lists, defaultList, onChange }: { step: StepIo; fields: FieldOut[]; members: Member[]; lists: { id: string; name: string }[]; defaultList?: string; onChange: (c: Record<string, unknown>) => void }) {
  const t = useTranslations('automations');
  const c = step.config as Record<string, unknown>;
  const targetList = (c.list_id as string) ?? defaultList;
  const list = body(useGetList(targetList ?? '', { query: { enabled: !!targetList && ['set_stage', 'create_card'].includes(step.action_type) } }).data);
  // What the form shows must be what gets saved: persist the default list right away.
  useEffect(() => {
    if (step.action_type === 'create_card' && !c.list_id && targetList) onChange({ ...c, list_id: targetList });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step.action_type, targetList]);
  switch (step.action_type) {
    case 'set_field': {
      const f = fields.find((x) => x.slug === c.field);
      const coerce = (v: string): unknown => (f && ['number', 'percentage', 'currency'].includes(f.type) ? (v === '' ? null : Number(v)) : f?.type === 'checkbox' ? v === 'true' : f && ['user', 'dropdown', 'label'].includes(f.type) && f.multiple ? [v] : v);
      const opts = f ? (f.type === 'user' ? members.map((m) => ({ value: m.user_id, label: fullName(m) })) : itemsOf(f).map((o) => ({ value: o, label: o }))) : [];
      return (
        <div className="grid gap-2 sm:grid-cols-2">
          <SimpleSelect label={t('field')} value={(c.field as string) ?? null} onChange={(v) => v && onChange({ field: v })} options={fields.filter((x) => !['rich', 'card'].includes(x.type)).map((x) => ({ value: x.slug, label: x.name }))} />
          {opts.length > 0 ? <SimpleSelect label={t('value')} value={Array.isArray(c.value) ? (c.value[0] as string) : ((c.value as string) ?? null)} onChange={(v) => v && onChange({ ...c, value: coerce(v) })} options={opts} /> : <TextInput label={t('value')} value={c.value === undefined || c.value === null ? '' : String(c.value)} onChange={(v) => onChange({ ...c, value: coerce(v) })} />}
        </div>
      );
    }
    case 'set_stage':
      return (
        <div className="grid gap-2 sm:grid-cols-2">
          <SimpleSelect label={t('list')} value={targetList ?? null} onChange={(v) => v && onChange({ list_id: v })} options={lists.map((l) => ({ value: l.id, label: l.name }))} />
          <SimpleSelect label={t('stage')} value={(c.stage_id as string) ?? null} onChange={(v) => v && onChange({ ...c, stage_id: v })} options={(list?.stages ?? []).map((s) => ({ value: s.id, label: s.name }))} testId="set-stage" />
        </div>
      );
    case 'change_assignee':
      return (
        <div className="grid gap-2 sm:grid-cols-2">
          <SimpleSelect label={t('add')} value={((c.add as string[]) ?? [])[0] ?? null} allowEmpty onChange={(v) => onChange({ ...c, add: v && v !== '__none' ? [v] : [] })} options={members.map((m) => ({ value: m.user_id, label: fullName(m) }))} />
          <SimpleSelect label={t('remove')} value={((c.remove as string[]) ?? [])[0] ?? null} allowEmpty onChange={(v) => onChange({ ...c, remove: v && v !== '__none' ? [v] : [] })} options={members.map((m) => ({ value: m.user_id, label: fullName(m) }))} />
        </div>
      );
    case 'create_comment':
      return <TextAreaInput label={t('comment')} rows={3} value={(c.content as string) ?? ''} onChange={(v) => onChange({ content: v })} maxLength={20000} />;
    case 'create_card':
      return (
        <div className="grid gap-2 sm:grid-cols-3">
          <SimpleSelect label={t('list')} value={targetList ?? null} onChange={(v) => v && onChange({ ...c, list_id: v })} options={lists.map((l) => ({ value: l.id, label: l.name }))} />
          <SimpleSelect label={t('stage')} value={(c.stage_id as string) ?? null} allowEmpty onChange={(v) => onChange({ ...c, stage_id: v && v !== '__none' ? v : undefined })} options={(list?.stages ?? []).map((s) => ({ value: s.id, label: s.name }))} />
          <TextInput label={t('cardTitle')} value={String(((c.data as Record<string, unknown>) ?? {}).title ?? '')} onChange={(v) => onChange({ ...c, data: { title: v } })} maxLength={500} />
        </div>
      );
    case 'send_webhook':
      return (
        <div className="flex flex-col gap-2">
          <TextInput label={t('url')} value={(c.url as string) ?? ''} onChange={(v) => onChange({ ...c, url: v })} placeholder="https://hooks.example.com/…" maxLength={2000} />
          <TextAreaInput label={t('payload')} rows={3} value={typeof c.body === 'string' ? c.body : c.body ? JSON.stringify(c.body, null, 2) : ''} onChange={(v) => { let parsed: unknown = v; try { parsed = v.trim() ? JSON.parse(v) : undefined; } catch { /* keep the raw text until it is valid JSON */ } onChange({ ...c, body: parsed }); }} />
          <p className="text-xs text-muted">{t('payloadHint', { example: '{{trigger.card.key}}' })}</p>
        </div>
      );
    case 'send_email':
      return (
        <div className="flex flex-col gap-2">
          <SimpleSelect label={t('to')} value={(c.to as string) ?? null} onChange={(v) => v && onChange({ ...c, to: v })} options={members.map((m) => ({ value: m.user_id, label: fullName(m) }))} testId="email-to" />
          <TextInput label={t('subject')} value={(c.subject as string) ?? ''} onChange={(v) => onChange({ ...c, subject: v })} maxLength={200} />
          <TextAreaInput label={t('emailBody')} rows={4} value={(c.body as string) ?? ''} onChange={(v) => onChange({ ...c, body: v })} maxLength={20000} />
        </div>
      );
    case 'create_sub_card':
      return <TextInput label={t('cardTitle')} value={String(((c.data as Record<string, unknown>) ?? {}).title ?? '')} onChange={(v) => onChange({ ...c, data: { title: v } })} maxLength={500} />;
    default:
      return null;
  }
}
