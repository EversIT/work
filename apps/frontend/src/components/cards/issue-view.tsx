'use client';

import { useState } from 'react';
import {
  Button,
  Dropdown,
  Modal,
  Spinner,
  buttonVariants,
  toast,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import {
  ChevronDown,
  ChevronRight,
  Eye,
  EyeOff,
  Link2,
  MoreHorizontal,
  Plus,
  Trash2,
} from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import Link from 'next/link';
import {
  useDeleteCard,
  useGetCard,
  useGetCardChildren,
  usePutCardList,
  useUpdateCard,
  putCardSprint,
} from '../../api/generated/cards/cards';
import type { FieldSuggestion } from '../../api/generated/model';
import { useGetList } from '../../api/generated/lists/lists';
import { useListSprints } from '../../api/generated/sprints/sprints';
import {
  useAddWatcher,
  useIsWatching,
  useRemoveWatcher,
} from '../../api/generated/notifications/notifications';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { asStrings } from '../../lib/cards';
import { issueTitle, issueType } from '../../lib/issue';
import { BASE_PATH, routes } from '../../lib/routes';
import { IssueTypeIcon, Lozenge, stageKind } from '../jira/issue-ui';
import { useCreateIssue } from '../shell/create-issue-context';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { JiraDialog } from '../ui/jira-dialog';
import { Panel } from '../ui/settings';
import { SimpleSelect } from '../ui/inputs';
import { AiAssistant } from './ai-assistant';
import { InlineSuggestion, SuggestFieldsButton } from './ai-details';
import { CrmHeader } from './crm-header';
import { CollabField } from '../rich/collab-field';
import { FieldEditor } from './field-editor';
import { IssueActivity } from './issue-activity';
import { InlineTitle } from './inline-title';
import { LinkedIssues } from './linked-issues';

/** Details-panel order, like Jira's right-hand column. */
const DETAIL_ORDER = [
  'assignee',
  'reporter',
  'priority',
  'labels',
  'story_points',
  'due_at',
  'starts_at',
  'estimate_hours',
];
const rank = (slug: string) =>
  DETAIL_ORDER.indexOf(slug) === -1 ? 99 : DETAIL_ORDER.indexOf(slug);

export function IssueView({
  cardId,
  variant,
  onClose,
}: {
  cardId: string;
  workspaceId: string;
  variant: 'modal' | 'page';
  onClose: () => void;
}) {
  const t = useTranslations('issue');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const ctx = useWorkspaceCtx();
  const createIssue = useCreateIssue();
  const card = body(useGetCard(cardId).data);
  const type = card ? issueType(card, ctx.cardTypes) : undefined;
  const fields = type?.fields ?? [];
  const title = fields.find((f) => f.is_title);
  const description = fields.find((f) => f.is_description);
  const details = fields
    .filter(
      (f) =>
        !f.is_title && !f.is_description && !['stage', 'card'].includes(f.type),
    )
    .sort((a, b) => rank(a.slug) - rank(b.slug));
  const update = useUpdateCard();
  const canWrite =
    ctx.members.find((m) => m.user_id === ctx.me?.id)?.role !== 'viewer';
  const del = useDeleteCard();
  // Values suggested for empty fields, shown under the field until accepted.
  const [suggested, setSuggested] = useState<FieldSuggestion[]>([]);
  const putList = usePutCardList();
  const children = body(useGetCardChildren(cardId).data) ?? [];
  const primary = card?.lists[0];
  const list = body(
    useGetList(primary?.list_id ?? '', { query: { enabled: !!primary } }).data,
  );
  const sprints =
    body(
      useListSprints(primary?.list_id ?? '', {
        query: { enabled: !!primary && list?.methodology === 'scrum' },
      }).data,
    ) ?? [];
  const watching =
    body(useIsWatching({ resource_type: 'card', resource_id: cardId }).data)
      ?.watching ?? false;
  const addWatcher = useAddWatcher();
  const removeWatcher = useRemoveWatcher();
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));

  const content = (() => {
    if (!card || !type) {
      return (
        <div className="flex justify-center p-12">
          <Spinner aria-label={t('loading')} />
        </div>
      );
    }
    const patch = async (slug: string, value: unknown) => {
      try {
        await update.mutateAsync({
          id: card.id,
          data: { data: { [slug]: value } },
        });
        await qc.invalidateQueries();
      } catch (e) {
        fail(e);
      }
    };
    const stage = list?.stages.find((s) => s.id === primary?.list_stage_id);
    const done = children.filter((c) => {
      const cl = c.lists[0];
      return list?.stages.find((s) => s.id === cl?.list_stage_id)?.is_completed;
    }).length;
    const assigneeField = details.find((f) => f.is_assignee);
    const mine = ctx.me?.id;
    const assignedToMe =
      !!assigneeField &&
      !!mine &&
      asStrings(card.data[assigneeField.slug]).includes(mine);

    return (
      <div
        className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_22rem]"
        data-testid="card-detail"
      >
        <div className="flex min-w-0 flex-col gap-6">
          {type.layout !== 'default' && (
            <CrmHeader
              card={card}
              type={type}
              title={issueTitle(card, ctx.cardTypes)}
            />
          )}
          {title && (
            <div data-testid="issue-title" className="-mx-2">
              <InlineTitle
                label={t('summary')}
                value={
                  typeof card.data[title.slug] === 'string'
                    ? (card.data[title.slug] as string)
                    : ''
                }
                disabled={!canWrite}
                onCommit={(v) => patch(title.slug, v)}
              />
            </div>
          )}
          {type.icon !== 'subtask' && canWrite && (
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="secondary"
                onPress={() =>
                  createIssue.open({
                    listId: primary?.list_id,
                    parentId: card.id,
                    sprintId: primary?.sprint_id,
                  })
                }
                data-testid="add-child"
              >
                <Plus size={14} />
                {t('addChild')}
              </Button>
            </div>
          )}
          {description && (
            <section data-testid="issue-description">
              {description.type === 'rich' ? (
                <CollabField
                  label={t('description')}
                  cardId={card.id}
                  slug={description.slug}
                  value={card.data[description.slug]}
                  canWrite={canWrite}
                  onSave={async (doc) => {
                    // `collab_keep`: this save belongs to the live editing session, so it stays open.
                    await update.mutateAsync({
                      id: card.id,
                      data: {
                        data: { [description.slug]: doc },
                        collab_keep: [description.slug],
                      },
                    });
                    await qc.invalidateQueries({ queryKey: ['cards'] });
                  }}
                />
              ) : (
                <FieldEditor
                  field={description}
                  label={t('description')}
                  value={card.data[description.slug]}
                  members={ctx.members}
                  onCommit={(v) => patch(description.slug, v)}
                />
              )}
            </section>
          )}
          {description?.type === 'rich' && canWrite && (
            <div className="-mt-3 flex flex-wrap gap-2">
              <AiAssistant
                shortcut={card.data[description.slug] ? 'improve' : 'draft'}
                card={card}
                canWrite={canWrite}
                descriptionSlug={description.slug}
                titleSlug={title?.slug}
                title={
                  title && typeof card.data[title.slug] === 'string'
                    ? (card.data[title.slug] as string)
                    : ''
                }
                kind={type?.name}
              />
            </div>
          )}
          {children.length > 0 && (
            <section aria-label={t('childIssues')}>
              <div className="mb-2 flex items-center justify-between">
                <h3 className="text-base font-semibold">{t('childIssues')}</h3>
                <span className="text-xs text-muted">
                  {t('childProgress', { done, total: children.length })}
                </span>
              </div>
              <div
                className="mb-3 h-1.5 overflow-hidden rounded-full bg-default"
                aria-hidden
              >
                <div
                  className="h-full rounded-full bg-success transition-all duration-500"
                  style={{ width: `${(done / children.length) * 100}%` }}
                />
              </div>
              <ul className="divide-y divide-separator overflow-hidden rounded-xl border border-separator">
                {children.map((c) => {
                  const ct = issueType(c, ctx.cardTypes);
                  const st = list?.stages.find(
                    (s) => s.id === c.lists[0]?.list_stage_id,
                  );
                  return (
                    <li
                      key={c.id}
                      className="flex items-center gap-3 px-3 py-2 text-sm transition-colors hover:bg-surface-secondary"
                    >
                      <IssueTypeIcon icon={ct?.icon} color={ct?.color} />
                      <span className="w-20 shrink-0 text-xs text-muted">
                        {c.key}
                      </span>
                      <Link
                        className="min-w-0 flex-1 truncate hover:underline"
                        href={routes.browse(c.key)}
                      >
                        {issueTitle(c, ctx.cardTypes)}
                      </Link>
                      {st && list && (
                        <Lozenge
                          kind={stageKind(st, list.stages[0]?.id === st.id)}
                        >
                          {st.name}
                        </Lozenge>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          )}
          <LinkedIssues
            cardId={card.id}
            canWrite={canWrite}
            stages={list?.stages}
            listId={primary?.list_id}
          />
          <IssueActivity
            cardId={card.id}
            projectId={ctx.workspace?.project_id}
          />
        </div>

        <aside className="flex flex-col gap-4 lg:sticky lg:top-4 lg:self-start">
          {list && stage && (
            <Dropdown>
              <Dropdown.Trigger
                className={buttonVariants({
                  variant: 'secondary',
                  className: 'h-10 w-full justify-between px-3',
                })}
                data-testid="status-button"
                aria-label={t('status')}
              >
                <Lozenge
                  kind={stageKind(stage, list.stages[0]?.id === stage.id)}
                  className="gap-1 px-2 py-1 text-xs"
                >
                  {stage.name}
                </Lozenge>
                <ChevronDown size={14} className="text-muted" />
              </Dropdown.Trigger>
              <Dropdown.Popover>
                <Dropdown.Menu
                  aria-label={t('status')}
                  onAction={async (key) => {
                    try {
                      await putList.mutateAsync({
                        id: card.id,
                        listId: list.id,
                        data: { stage_id: String(key) },
                      });
                      await qc.invalidateQueries();
                    } catch (e) {
                      fail(e);
                    }
                  }}
                >
                  {list.stages.map((s, i) => (
                    <Dropdown.Item key={s.id} id={s.id} textValue={s.name}>
                      <Lozenge kind={stageKind(s, i === 0)}>{s.name}</Lozenge>
                    </Dropdown.Item>
                  ))}
                </Dropdown.Menu>
              </Dropdown.Popover>
            </Dropdown>
          )}

          <Panel testId="issue-details">
            <Panel.Header className="py-3">
              <Panel.Title>{t('details')}</Panel.Title>
              {canWrite && (
                <SuggestFieldsButton cardId={card.id} onResult={setSuggested} />
              )}
            </Panel.Header>
            <div className="inline-fields flex flex-col py-1.5">
              {details.map((f) => (
                <DetailRow key={f.id} label={f.name}>
                  <FieldEditor
                    field={f}
                    value={card.data[f.slug]}
                    members={ctx.members}
                    hideLabel
                    onCommit={(v) => patch(f.slug, v)}
                  />
                  {canWrite &&
                    !card.data[f.slug] &&
                    suggested
                      .filter((x) => x.slug === f.slug)
                      .map((x) => (
                        <InlineSuggestion
                          key={x.slug}
                          suggestion={x}
                          onAccept={() => {
                            setSuggested((all) =>
                              all.filter((y) => y.slug !== x.slug),
                            );
                            void patch(f.slug, x.value);
                          }}
                        />
                      ))}
                  {f.is_assignee && canWrite && mine && !assignedToMe && (
                    <button
                      type="button"
                      onClick={() => patch(f.slug, f.multiple ? [mine] : mine)}
                      className="mt-0.5 px-3 text-xs text-accent hover:underline"
                      data-testid="assign-to-me"
                    >
                      {t('assignToMe')}
                    </button>
                  )}
                </DetailRow>
              ))}
              {primary && list?.methodology === 'scrum' && (
                <DetailRow label={t('sprint')}>
                  <SimpleSelect
                    hideLabel
                    label={t('sprint')}
                    value={primary.sprint_id ?? '__backlog'}
                    onChange={async (v) => {
                      try {
                        await putCardSprint(card.id, {
                          list_id: primary.list_id,
                          sprint_id: !v || v === '__backlog' ? null : v,
                        });
                        await qc.invalidateQueries();
                      } catch (e) {
                        fail(e);
                      }
                    }}
                    options={[
                      { value: '__backlog', label: t('backlog') },
                      ...sprints
                        .filter(
                          (s) =>
                            s.state !== 'completed' ||
                            s.id === primary.sprint_id,
                        )
                        .map((s) => ({ value: s.id, label: s.name })),
                    ]}
                    testId="sprint-select"
                  />
                </DetailRow>
              )}
            </div>
          </Panel>
          <p className="px-1 text-xs text-muted">
            {t('createdAt', {
              date: formatDate(card.created_at, locale, true),
            })}
            <br />
            {t('updatedAt', {
              date: formatDate(card.updated_at, locale, true),
            })}
          </p>
        </aside>
      </div>
    );
  })();

  const actions = card && (
    <div className="flex items-center gap-1">
      <AiAssistant
        card={card}
        canWrite={canWrite}
        titleSlug={title?.slug}
        descriptionSlug={
          description?.type === 'rich' ? description.slug : undefined
        }
      />
      <Button
        size="sm"
        variant="tertiary"
        aria-pressed={watching}
        data-testid="watch"
        onPress={async () => {
          try {
            if (watching)
              await removeWatcher.mutateAsync({
                params: { resource_type: 'card', resource_id: card.id },
              });
            else
              await addWatcher.mutateAsync({
                data: { resource_type: 'card', resource_id: card.id },
              });
            await qc.invalidateQueries();
          } catch (e) {
            fail(e);
          }
        }}
      >
        {watching ? <EyeOff size={14} /> : <Eye size={14} />}
        {watching ? t('unwatch') : t('watch')}
      </Button>
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={t('copyLink')}
        onPress={() =>
          navigator.clipboard
            ?.writeText(
              `${window.location.origin}${BASE_PATH}${routes.browse(card.key)}`,
            )
            .then(() => toast.success(t('linkCopied')))
        }
      >
        <Link2 size={16} />
      </Button>
      <Dropdown>
        <Dropdown.Trigger
          className={buttonVariants({
            variant: 'ghost',
            isIconOnly: true,
            size: 'sm',
          })}
          aria-label={t('more')}
          data-testid="issue-menu"
        >
          <MoreHorizontal size={16} />
        </Dropdown.Trigger>
        <Dropdown.Popover>
          <Dropdown.Menu
            aria-label={t('more')}
            onAction={async (key) => {
              if (key !== 'delete') return;
              if (!window.confirm(t('confirmDelete'))) return;
              try {
                await del.mutateAsync({ id: card.id });
                await qc.invalidateQueries();
                onClose();
              } catch (e) {
                fail(e);
              }
            }}
          >
            <Dropdown.Item id="delete" textValue={t('delete')} variant="danger">
              <span
                className="flex items-center gap-2"
                data-testid="delete-card"
              >
                <Trash2 size={14} />
                {t('delete')}
              </span>
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );

  const wsKey = ctx.workspace?.key_prefix;
  const listBrief = ctx.lists.find((l) => l.id === primary?.list_id);
  const crumb = card && type && (
    <nav
      aria-label={t('breadcrumb')}
      className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm text-muted"
    >
      {variant === 'page' && wsKey && (
        <>
          <Link href={routes.workspace(wsKey)} className="hover:underline">
            {ctx.workspace?.name}
          </Link>
          <ChevronRight size={12} aria-hidden />
          {listBrief && (
            <>
              <Link
                href={routes.project(wsKey, listBrief.slug)}
                className="hover:underline"
              >
                {listBrief.name}
              </Link>
              <ChevronRight size={12} aria-hidden />
            </>
          )}
        </>
      )}
      <IssueTypeIcon icon={type.icon} color={type.color} title={type.name} />
      <Link
        href={routes.browse(card.key)}
        className="font-medium hover:underline"
        data-testid="issue-key"
      >
        {card.key}
      </Link>
    </nav>
  );

  if (variant === 'page') {
    return (
      <div className="mx-auto max-w-6xl p-6">
        <div className="mb-5 flex items-center justify-between gap-4">
          {crumb}
          {actions}
        </div>
        {content}
      </div>
    );
  }
  return (
    <JiraDialog
      isOpen
      onClose={onClose}
      title={card ? `${card.key}` : t('loading')}
      size="xl"
      testId="issue-dialog"
      header={
        <div className="flex items-center justify-between px-6 pb-2 pt-4">
          {crumb ?? <span />}
          <div className="flex items-center gap-1">
            {actions}
            <Modal.CloseTrigger className="static ml-1" />
          </div>
        </div>
      }
    >
      {content}
    </JiraDialog>
  );
}

/** Label on the left, value on the right: one line of the details panel. */
function DetailRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] items-start gap-1 px-3 py-0.5">
      <span
        className="truncate pt-2.5 text-xs font-medium text-muted"
        title={label}
      >
        {label}
      </span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}
