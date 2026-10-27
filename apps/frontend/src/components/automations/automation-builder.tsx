'use client';

import { routes } from '../../lib/routes';
import { Button, Dropdown, Switch, buttonVariants, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import {
  ArrowLeft,
  ArrowRightLeft,
  CalendarClock,
  ChevronDown,
  ChevronUp,
  Columns3,
  FilePlus2,
  Flag,
  GripVertical,
  ListTree,
  Mail,
  MessageSquare,
  Pencil,
  Play,
  SlidersHorizontal,
  Plus,
  Trash2,
  UserRoundPlus,
  Webhook,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useMemo, useState } from 'react';
import {
  useCreateAutomation,
  useGetAutomation,
  useUpdateAutomation,
} from '../../api/generated/automations/automations';
import type { AutomationOut, FieldOut } from '../../api/generated/model';
import { useGetWorkspaceTree } from '../../api/generated/spaces/spaces';
import { body, errorKind, serverMessage } from '../../lib/api';
import { useCardTypes, useMembers, useWorkspace } from '../../lib/hooks';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { SettingsSkeleton } from '../ui/settings';
import {
  ACTIONS,
  StepConfig,
  TRIGGERS,
  TriggerConfig,
  type Draft,
} from './parts';

const ACTION_ICON: Record<string, LucideIcon> = {
  set_field: Pencil,
  set_stage: Columns3,
  change_assignee: UserRoundPlus,
  create_comment: MessageSquare,
  create_card: FilePlus2,
  create_sub_card: ListTree,
  send_webhook: Webhook,
  send_email: Mail,
};
const TRIGGER_ICON: Record<string, LucideIcon> = {
  card_created: FilePlus2,
  field_updated: Pencil,
  stage_changed: ArrowRightLeft,
  comment_created: MessageSquare,
  assignee_added: UserRoundPlus,
  assignee_removed: UserRoundPlus,
  date_arrives: CalendarClock,
  webhook: Webhook,
};

let keySeq = 0;
const nextKey = () => `s${++keySeq}`;

const STEP = 'application/x-work-step';
const NEW_ACTION = 'application/x-work-action';

const blankStep = (action: string) => ({
  action_type: action,
  config: action === 'create_comment' ? { content: '' } : {},
});

type Selection = 'trigger' | 'rule' | number;

/**
 * The page where an automation is built: the rule as a flow on a canvas (a trigger, then actions
 * in order), a palette to take actions from, and a panel to set up whatever is selected. Actions
 * are put in place and re-ordered by dragging them; the same can be done with the buttons.
 */
export function AutomationBuilder({
  workspaceId,
  id,
  listId,
}: {
  workspaceId: string;
  /** `new`, or the id of the rule. */
  id: string;
  /** The project the rule was opened from; it is the scope of a new rule. */
  listId?: string;
}) {
  const existing = useGetAutomation(id, { query: { enabled: id !== 'new' } });
  const rule = body(existing.data);
  if (id !== 'new' && !rule)
    return (
      <div className="mx-auto max-w-3xl p-6">
        <SettingsSkeleton />
      </div>
    );
  return (
    <BuilderInner
      key={rule?.id ?? 'new'}
      workspaceId={workspaceId}
      rule={rule}
      listId={listId}
    />
  );
}

function BuilderInner({
  workspaceId,
  rule,
  listId,
}: {
  workspaceId: string;
  rule?: AutomationOut;
  listId?: string;
}) {
  const t = useTranslations('automations');
  const te = useTranslations('errors');
  const router = useRouter();
  const qc = useQueryClient();
  const workspace = useWorkspace(workspaceId);
  const members = useMembers(workspace?.project_id);
  const types = useCardTypes(workspaceId);
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  const create = useCreateAutomation();
  const update = useUpdateAutomation();
  const lists = useMemo(
    () => tree?.spaces.flatMap((s) => s.lists) ?? [],
    [tree],
  );
  const fields: FieldOut[] = types
    .flatMap((c) => c.fields)
    .filter((f, i, a) => a.findIndex((x) => x.slug === f.slug) === i);

  const initial: Draft = useMemo(
    () =>
      rule
        ? {
            name: rule.name,
            trigger_type: rule.trigger_type,
            trigger_config: (rule.trigger_config ?? {}) as Record<
              string,
              unknown
            >,
            steps: rule.steps,
            list:
              rule.locations.find((l) => l.location_type === 'list')
                ?.location_id ?? null,
          }
        : {
            name: '',
            trigger_type: 'card_created',
            trigger_config: {},
            steps: [blankStep('create_comment')],
            list: listId ?? null,
          },
    [rule, listId],
  );
  const [draft, setDraft] = useState<Draft>(initial);
  // Stable keys, so a step keeps its identity (and does not replay its entrance) when moved.
  const [keys, setKeys] = useState<string[]>(() =>
    initial.steps.map(() => nextKey()),
  );
  const [enabled, setEnabled] = useState(rule?.is_enabled ?? true);
  const [selected, setSelected] = useState<Selection>(
    rule ? 'trigger' : 'rule',
  );
  const [saved, setSaved] = useState(
    JSON.stringify({ d: initial, e: rule?.is_enabled ?? true }),
  );
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);

  const dirty = JSON.stringify({ d: draft, e: enabled }) !== saved;
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [dirty]);

  const listName = lists.find((l) => l.id === draft.list);
  const wsKey = workspace?.key_prefix ?? '';
  const settingsPath = (() => {
    const l = lists.find((x) => x.id === (listId ?? draft.list));
    return l
      ? routes.projectSettings(wsKey, l.slug, 'automations')
      : routes.workspaceSettings(wsKey, 'automations');
  })();

  // ---- edits to the flow ----
  const insertStep = (at: number, action: string) => {
    setDraft((d) => ({
      ...d,
      steps: [...d.steps.slice(0, at), blankStep(action), ...d.steps.slice(at)],
    }));
    setKeys((k) => [...k.slice(0, at), nextKey(), ...k.slice(at)]);
    setSelected(at);
  };
  const moveStep = (from: number, to: number) => {
    // `to` is a position in the list as it was; moving down shifts it by one.
    const target = from < to ? to - 1 : to;
    if (target === from || target < 0) return;
    const reorder = <T,>(a: T[]) => {
      const c = [...a];
      const [x] = c.splice(from, 1);
      c.splice(target, 0, x);
      return c;
    };
    setDraft((d) => ({ ...d, steps: reorder(d.steps) }));
    setKeys(reorder);
    setSelected(target);
  };
  const removeStep = (i: number) => {
    setDraft((d) => ({ ...d, steps: d.steps.filter((_, j) => j !== i) }));
    setKeys((k) => k.filter((_, j) => j !== i));
    setSelected('trigger');
  };
  const setStep = (i: number, step: Draft['steps'][number]) =>
    setDraft((d) => ({
      ...d,
      steps: d.steps.map((x, j) => (j === i ? step : x)),
    }));

  const save = async () => {
    setError(null);
    if (!draft.name.trim()) {
      setSelected('rule');
      setError(t('nameRequired'));
      return;
    }
    const payload = {
      name: draft.name.trim(),
      trigger_type: draft.trigger_type,
      trigger_config: draft.trigger_config,
      steps: draft.steps,
      locations: draft.list
        ? [{ location_type: 'list', location_id: draft.list }]
        : [],
    };
    try {
      if (rule) {
        await update.mutateAsync({
          id: rule.id,
          data: { ...payload, is_enabled: enabled },
        });
        setSaved(JSON.stringify({ d: draft, e: enabled }));
        await qc.invalidateQueries();
        toast.success(t('savedRule'));
      } else {
        const res = await create.mutateAsync({
          id: workspaceId,
          data: { ...payload, is_enabled: enabled },
        });
        setSaved(JSON.stringify({ d: draft, e: enabled }));
        await qc.invalidateQueries();
        toast.success(t('savedRule'));
        if (res.status === 200)
          router.replace(routes.automation(wsKey, res.data.id, listId));
      }
    } catch (e) {
      setError(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  // ---- drag and drop ----
  const dropAt = (index: number, e: React.DragEvent) => {
    e.preventDefault();
    const step = e.dataTransfer.getData(STEP);
    const action = e.dataTransfer.getData(NEW_ACTION);
    setOver(null);
    setDragging(null);
    if (step !== '') moveStep(Number(step), index);
    else if (action) insertStep(index, action);
  };
  const acceptsDrop = (e: React.DragEvent) =>
    e.dataTransfer.types.includes(STEP) ||
    e.dataTransfer.types.includes(NEW_ACTION);

  const sel = selected;
  const summarise = (s: Draft['steps'][number]): string => {
    const c = s.config as Record<string, unknown>;
    switch (s.action_type) {
      case 'set_field':
        return c.field ? `${String(c.field)} = ${String(c.value ?? '')}` : '';
      case 'create_comment':
        return String(c.content ?? '').slice(0, 80);
      case 'create_card':
      case 'create_sub_card':
        return String(((c.data as Record<string, unknown>) ?? {}).title ?? '');
      case 'send_webhook':
        try {
          return new URL(String(c.url ?? '')).host;
        } catch {
          return '';
        }
      case 'send_email':
        return String(c.subject ?? '');
      default:
        return '';
    }
  };

  return (
    <div
      className="flex min-h-[calc(100vh-3.5rem)] flex-col"
      data-testid="automation-builder"
    >
      {/* Top bar: where you are, the name, on/off, and the one button that saves. */}
      <header className="sticky top-0 z-30 flex flex-wrap items-center gap-3 border-b border-separator bg-surface px-4 py-2.5 md:px-6">
        <Link
          href={settingsPath}
          data-testid="builder-back"
          className="flex h-8 items-center gap-2 rounded-md px-2 text-sm text-muted transition-colors hover:bg-default hover:text-foreground"
        >
          <ArrowLeft size={14} />
          {t('backToRules')}
        </Link>
        <span className="mx-1 hidden h-5 w-px bg-separator md:block" />
        <div className="min-w-48 flex-1 md:max-w-md">
          <TextInput
            label={t('ruleName')}
            value={draft.name}
            onChange={(v) => setDraft({ ...draft, name: v })}
            maxLength={100}
            placeholder={t('untitled')}
          />
        </div>
        <div className="ml-auto flex items-center gap-4">
          <Button
            size="sm"
            variant={sel === 'rule' ? 'secondary' : 'tertiary'}
            onPress={() => setSelected('rule')}
            data-testid="rule-settings"
          >
            <SlidersHorizontal size={14} />
            {t('ruleSettings')}
          </Button>
          {dirty && (
            <span
              className="text-xs text-muted settings-enter"
              data-testid="builder-dirty"
            >
              {t('unsavedRule')}
            </span>
          )}
          <Switch
            isSelected={enabled}
            onChange={setEnabled}
            className="flex flex-row items-center gap-2"
          >
            <Switch.Control>
              <Switch.Thumb />
            </Switch.Control>
            <Switch.Content>
              <span className="text-sm">{enabled ? t('on') : t('off')}</span>
            </Switch.Content>
          </Switch>
          <Button
            onPress={save}
            isPending={create.isPending || update.isPending}
            isDisabled={!dirty && !!rule}
            data-testid="builder-save"
          >
            {t('save')}
          </Button>
        </div>
      </header>
      {error && (
        <p
          role="alert"
          className="bg-danger/10 px-6 py-2 text-sm text-danger settings-enter"
          data-testid="builder-error"
        >
          {error}
        </p>
      )}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[16rem_minmax(0,1fr)_22rem]">
        {/* Palette */}
        <aside className="hidden flex-col gap-5 overflow-y-auto border-r border-separator bg-surface-secondary p-4 lg:flex">
          <div>
            <p className="jira-eyebrow pb-2">{t('when')}</p>
            <ul className="flex flex-col gap-1">
              {TRIGGERS.map((x) => {
                const Icon = TRIGGER_ICON[x] ?? Zap;
                return (
                  <li key={x}>
                    <button
                      type="button"
                      onClick={() => {
                        setDraft({
                          ...draft,
                          trigger_type: x,
                          trigger_config: {},
                        });
                        setSelected('trigger');
                      }}
                      aria-pressed={draft.trigger_type === x}
                      className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors ${
                        draft.trigger_type === x
                          ? 'bg-accent-soft font-medium text-accent'
                          : 'text-foreground hover:bg-default'
                      }`}
                    >
                      <Icon size={14} />
                      {t(`trigger.${x}`)}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
          <div>
            <p className="jira-eyebrow pb-1">{t('then')}</p>
            <p className="pb-2 text-xs text-muted">{t('paletteHint')}</p>
            <ul className="flex flex-col gap-1" data-testid="palette">
              {ACTIONS.map((x) => {
                const Icon = ACTION_ICON[x] ?? Play;
                return (
                  <li key={x}>
                    <div
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData(NEW_ACTION, x);
                        e.dataTransfer.effectAllowed = 'copy';
                      }}
                      onDragEnd={() => setOver(null)}
                      data-testid={`palette-${x}`}
                      className="group relative flex cursor-grab items-center gap-2 rounded-md border border-transparent px-2 py-1.5 text-sm text-foreground transition-all hover:border-separator hover:bg-surface active:cursor-grabbing"
                    >
                      <GripVertical
                        size={12}
                        className="text-muted"
                        aria-hidden
                      />
                      <span className="flex size-6 items-center justify-center rounded-md bg-success/10 text-success">
                        <Icon size={13} />
                      </span>
                      <span className="flex-1">{t(`action.${x}`)}</span>
                      <Button
                        isIconOnly
                        size="sm"
                        variant="ghost"
                        aria-label={`${t('addAction')}: ${t(`action.${x}`)}`}
                        onPress={() => insertStep(draft.steps.length, x)}
                        data-testid={`palette-add-${x}`}
                        className="absolute right-1 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                      >
                        <Plus size={14} />
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        </aside>

        {/* Canvas */}
        <main className="min-w-0 overflow-y-auto bg-surface-secondary px-4 py-8 [background-image:radial-gradient(var(--separator)_1px,transparent_1px)] [background-size:18px_18px]">
          <div className="mx-auto flex w-full max-w-lg flex-col items-stretch">
            <FlowNode
              tone="accent"
              label={t('when')}
              icon={TRIGGER_ICON[draft.trigger_type] ?? Zap}
              title={t(`trigger.${draft.trigger_type}`)}
              summary={listName ? `${listName.name}` : t('allLists')}
              selected={sel === 'trigger'}
              onSelect={() => setSelected('trigger')}
              testId="node-trigger"
            />
            {draft.steps.map((s, i) => (
              <div key={keys[i]} className="flex flex-col items-stretch">
                <Connector
                  index={i}
                  active={over === i && dragging !== i && dragging !== i - 1}
                  dragging={dragging !== null}
                  onAdd={(a) => insertStep(i, a)}
                  onOver={(e) => {
                    if (!acceptsDrop(e)) return;
                    e.preventDefault();
                    setOver(i);
                  }}
                  onLeave={() => setOver((o) => (o === i ? null : o))}
                  onDrop={(e) => dropAt(i, e)}
                />
                <FlowNode
                  tone="success"
                  label={`${t('then')} · ${t('step', { n: i + 1 })}`}
                  icon={ACTION_ICON[s.action_type] ?? Play}
                  title={t(`action.${s.action_type}`)}
                  summary={summarise(s)}
                  selected={sel === i}
                  onSelect={() => setSelected(i)}
                  draggable
                  isDragging={dragging === i}
                  onDragStart={(e) => {
                    e.dataTransfer.setData(STEP, String(i));
                    e.dataTransfer.effectAllowed = 'move';
                    setDragging(i);
                  }}
                  onDragEnd={() => {
                    setDragging(null);
                    setOver(null);
                  }}
                  onRemove={
                    draft.steps.length > 1 ? () => removeStep(i) : undefined
                  }
                  removeLabel={t('removeStep')}
                  testId={`node-step-${i}`}
                  animate
                />
              </div>
            ))}
            <Connector
              index={draft.steps.length}
              active={
                over === draft.steps.length &&
                dragging !== draft.steps.length - 1
              }
              dragging={dragging !== null}
              onAdd={(a) => insertStep(draft.steps.length, a)}
              onOver={(e) => {
                if (!acceptsDrop(e)) return;
                e.preventDefault();
                setOver(draft.steps.length);
              }}
              onLeave={() =>
                setOver((o) => (o === draft.steps.length ? null : o))
              }
              onDrop={(e) => dropAt(draft.steps.length, e)}
            />
            <div className="flex items-center justify-center">
              <span className="flex items-center gap-1.5 rounded-full border border-separator bg-surface px-3 py-1 text-xs text-muted">
                <Flag size={12} />
                {t('end')}
              </span>
            </div>
          </div>
        </main>

        {/* Configuration of whatever is selected */}
        <aside
          className="overflow-y-auto border-l border-separator bg-surface p-4"
          data-testid="builder-panel"
        >
          <div key={String(sel)} className="settings-enter flex flex-col gap-4">
            {sel === 'rule' && (
              <>
                <PanelTitle icon={Zap} title={t('ruleSettings')} />
                <SimpleSelect
                  label={t('where')}
                  value={draft.list}
                  allowEmpty
                  onChange={(v) =>
                    setDraft({ ...draft, list: v === '__none' ? null : v })
                  }
                  options={lists.map((l) => ({ value: l.id, label: l.name }))}
                  placeholder={t('allLists')}
                  testId="rule-scope"
                />
                <p className="text-xs text-muted">{t('scopeHelp')}</p>
                <p className="text-xs text-muted">
                  {t('placeholderHint', {
                    example: '{{trigger.card.data.title}}',
                  })}
                </p>
              </>
            )}
            {sel === 'trigger' && (
              <>
                <PanelTitle
                  icon={TRIGGER_ICON[draft.trigger_type] ?? Zap}
                  title={t('when')}
                />
                <SimpleSelect
                  label={t('triggerLabel')}
                  value={draft.trigger_type}
                  onChange={(v) =>
                    v &&
                    setDraft({ ...draft, trigger_type: v, trigger_config: {} })
                  }
                  options={TRIGGERS.map((x) => ({
                    value: x,
                    label: t(`trigger.${x}`),
                  }))}
                  testId="trigger-type"
                />
                <TriggerConfig
                  draft={draft}
                  fields={fields}
                  members={members}
                  listId={draft.list ?? lists[0]?.id}
                  onChange={(cfg) =>
                    setDraft({ ...draft, trigger_config: cfg })
                  }
                />
              </>
            )}
            {typeof sel === 'number' && draft.steps[sel] && (
              <>
                <PanelTitle
                  icon={ACTION_ICON[draft.steps[sel].action_type] ?? Play}
                  title={`${t('then')} · ${t('step', { n: sel + 1 })}`}
                />
                <SimpleSelect
                  label={t('actionLabel')}
                  value={draft.steps[sel].action_type}
                  onChange={(v) =>
                    v && setStep(sel, { action_type: v, config: {} })
                  }
                  options={ACTIONS.map((x) => ({
                    value: x,
                    label: t(`action.${x}`),
                  }))}
                  testId={`action-type-${sel}`}
                />
                <StepConfig
                  step={draft.steps[sel]}
                  fields={fields}
                  members={members}
                  lists={lists}
                  defaultList={draft.list ?? lists[0]?.id}
                  onChange={(config) =>
                    setStep(sel, { ...draft.steps[sel], config })
                  }
                />
                <p className="text-xs text-muted">
                  {t('placeholderHint', {
                    example: '{{trigger.card.data.title}}',
                  })}
                </p>
                <div className="flex gap-2 border-t border-separator pt-3">
                  <Button
                    size="sm"
                    variant="tertiary"
                    isDisabled={sel === 0}
                    onPress={() => moveStep(sel, sel - 1)}
                    aria-label={t('moveUp')}
                  >
                    <ChevronUp size={14} />
                    {t('moveUp')}
                  </Button>
                  <Button
                    size="sm"
                    variant="tertiary"
                    isDisabled={sel === draft.steps.length - 1}
                    onPress={() => moveStep(sel, sel + 2)}
                    aria-label={t('moveDown')}
                  >
                    <ChevronDown size={14} />
                    {t('moveDown')}
                  </Button>
                  {draft.steps.length > 1 && (
                    <Button
                      size="sm"
                      variant="danger-soft"
                      className="ml-auto"
                      onPress={() => removeStep(sel)}
                    >
                      <Trash2 size={14} />
                      {t('removeStep')}
                    </Button>
                  )}
                </div>
              </>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

function PanelTitle({
  icon: Icon,
  title,
}: {
  icon: LucideIcon;
  title: string;
}) {
  return (
    <div className="flex items-center gap-2 border-b border-separator pb-3">
      <span className="flex size-7 items-center justify-center rounded-md bg-accent-soft text-accent">
        <Icon size={14} />
      </span>
      <h2 className="text-sm font-semibold">{title}</h2>
    </div>
  );
}

/** One box of the flow: the trigger, or an action (which has a handle to drag it by). */
function FlowNode({
  tone,
  label,
  icon: Icon,
  title,
  summary,
  selected,
  onSelect,
  draggable,
  isDragging,
  onDragStart,
  onDragEnd,
  onRemove,
  removeLabel,
  testId,
  animate,
}: {
  tone: 'accent' | 'success';
  label: string;
  icon: LucideIcon;
  title: string;
  summary?: string;
  selected: boolean;
  onSelect: () => void;
  draggable?: boolean;
  isDragging?: boolean;
  onDragStart?: (e: React.DragEvent) => void;
  onDragEnd?: () => void;
  onRemove?: () => void;
  removeLabel?: string;
  testId: string;
  animate?: boolean;
}) {
  const edge = tone === 'accent' ? 'border-l-accent' : 'border-l-success';
  const tint =
    tone === 'accent'
      ? 'bg-accent-soft text-accent'
      : 'bg-success/10 text-success';
  return (
    <div
      data-testid={testId}
      className={`group relative flex items-center gap-1 rounded-xl border border-l-4 bg-surface py-1 pr-2 shadow-sm transition-all duration-150 ${edge} ${
        selected
          ? 'border-accent ring-2 ring-accent/30'
          : 'border-separator hover:shadow-md'
      } ${isDragging ? 'scale-[0.98] opacity-40' : ''} ${animate ? 'settings-enter' : ''}`}
    >
      {draggable ? (
        <span
          draggable
          onDragStart={(e) => {
            // The whole box travels, not just the handle.
            const box = e.currentTarget.parentElement;
            if (box) e.dataTransfer.setDragImage(box, 24, 24);
            onDragStart?.(e);
          }}
          onDragEnd={onDragEnd}
          data-testid={`${testId}-grip`}
          className="flex h-10 w-6 shrink-0 cursor-grab items-center justify-center text-muted transition-colors hover:text-foreground active:cursor-grabbing"
          title={label}
        >
          <GripVertical size={14} aria-hidden />
        </span>
      ) : (
        <span className="w-2 shrink-0" />
      )}
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={selected}
        data-testid={`${testId}-select`}
        className="flex min-w-0 flex-1 items-center gap-3 rounded-lg px-1 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-accent/50"
      >
        <span
          className={`flex size-9 shrink-0 items-center justify-center rounded-lg ${tint}`}
        >
          <Icon size={18} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[0.7rem] font-semibold uppercase tracking-wide text-muted">
            {label}
          </span>
          <span className="block truncate text-sm font-medium">{title}</span>
          {summary && (
            <span className="block truncate text-xs text-muted">{summary}</span>
          )}
        </span>
      </button>
      {onRemove && (
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={removeLabel}
          onPress={onRemove}
          className="opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
        >
          <Trash2 size={14} />
        </Button>
      )}
    </div>
  );
}

/**
 * The line between two boxes. It takes an action that is dragged onto it, grows to show where the
 * action will land, and has a "+" for those who would rather click.
 */
function Connector({
  index,
  active,
  dragging,
  onAdd,
  onOver,
  onLeave,
  onDrop,
}: {
  index: number;
  active: boolean;
  dragging: boolean;
  onAdd: (action: string) => void;
  onOver: (e: React.DragEvent) => void;
  onLeave: () => void;
  onDrop: (e: React.DragEvent) => void;
}) {
  const t = useTranslations('automations');
  return (
    <div
      onDragEnter={onOver}
      onDragOver={onOver}
      onDragLeave={onLeave}
      onDrop={onDrop}
      data-testid={`connector-${index}`}
      className={`relative flex flex-col items-center justify-center transition-all duration-150 ${
        active ? 'h-16' : dragging ? 'h-12' : 'h-9'
      }`}
    >
      <span
        className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${active ? 'bg-accent' : 'bg-separator'}`}
        aria-hidden
      />
      {/* The drop hint sits on top of the button instead of replacing it, so the element the
          pointer is over stays put for the whole drag. */}
      {active && (
        <span className="pointer-events-none absolute z-20 rounded-lg border-2 border-dashed border-accent bg-accent-soft px-4 py-1 text-xs font-medium text-accent settings-enter">
          {t('dropHere')}
        </span>
      )}
      <Dropdown>
        <Dropdown.Trigger
          className={`${buttonVariants({
            variant: 'secondary',
            isIconOnly: true,
            size: 'sm',
          })} relative z-10 transition-transform hover:scale-110 ${active ? 'opacity-0' : ''}`}
          aria-label={t('addAction')}
          data-testid={`add-action-${index}`}
        >
          <Plus size={14} />
        </Dropdown.Trigger>
        <Dropdown.Popover>
          <Dropdown.Menu
            aria-label={t('addAction')}
            onAction={(k) => onAdd(String(k))}
          >
            {ACTIONS.map((x) => (
              <Dropdown.Item
                key={x}
                id={x}
                textValue={t(`action.${x}`)}
                data-testid={`add-${x}-${index}`}
              >
                {t(`action.${x}`)}
              </Dropdown.Item>
            ))}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );
}
