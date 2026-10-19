'use client';

import { Button, Dropdown, Modal, Spinner, buttonVariants, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ChevronDown, Eye, EyeOff, Link2, MoreHorizontal, Plus, Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import Link from 'next/link';
import { useDeleteCard, useGetCard, useGetCardChildren, usePutCardList, useUpdateCard, putCardSprint } from '../../api/generated/cards/cards';
import { useGetList } from '../../api/generated/lists/lists';
import { useListSprints } from '../../api/generated/sprints/sprints';
import { useAddWatcher, useIsWatching, useRemoveWatcher } from '../../api/generated/notifications/notifications';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { issueTitle, issueType } from '../../lib/issue';
import { IssueTypeIcon, Lozenge, stageKind } from '../jira/issue-ui';
import { useCreateIssue } from '../shell/create-issue-context';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { JiraDialog } from '../ui/jira-dialog';
import { SimpleSelect } from '../ui/inputs';
import { FieldEditor } from './field-editor';
import { IssueActivity } from './issue-activity';

/** Details-panel order, like Jira's right-hand column. */
const DETAIL_ORDER = ['assignee', 'reporter', 'priority', 'labels', 'story_points', 'due_at', 'starts_at', 'estimate_hours'];
const rank = (slug: string) => (DETAIL_ORDER.indexOf(slug) === -1 ? 99 : DETAIL_ORDER.indexOf(slug));

export function IssueView({
  cardId,
  workspaceId,
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
  const details = fields.filter((f) => !f.is_title && !f.is_description && !['stage', 'card'].includes(f.type)).sort((a, b) => rank(a.slug) - rank(b.slug));
  const update = useUpdateCard();
  const del = useDeleteCard();
  const putList = usePutCardList();
  const children = body(useGetCardChildren(cardId).data) ?? [];
  const primary = card?.lists[0];
  const list = body(useGetList(primary?.list_id ?? '', { query: { enabled: !!primary } }).data);
  const sprints = body(useListSprints(primary?.list_id ?? '', { query: { enabled: !!primary && list?.methodology === 'scrum' } }).data) ?? [];
  const watching = body(useIsWatching({ resource_type: 'card', resource_id: cardId }).data)?.watching ?? false;
  const addWatcher = useAddWatcher();
  const removeWatcher = useRemoveWatcher();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));

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
        await update.mutateAsync({ id: card.id, data: { data: { [slug]: value } } });
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

    return (
      <div className="grid gap-6 md:grid-cols-[minmax(0,1fr)_20rem]" data-testid="card-detail">
        <div className="flex min-w-0 flex-col gap-5">
          {title && (
            <div data-testid="issue-title">
              <FieldEditor field={title} label={t('summary')} value={card.data[title.slug]} members={ctx.members} onCommit={(v) => patch(title.slug, v)} />
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            {type.icon !== 'subtask' && (
              <Button size="sm" variant="tertiary" onPress={() => createIssue.open({ listId: primary?.list_id, parentId: card.id, sprintId: primary?.sprint_id })} data-testid="add-child">
                <Plus size={14} />
                {t('addChild')}
              </Button>
            )}
          </div>
          {description && (
            <div>
              <FieldEditor field={description} label={t('description')} value={card.data[description.slug]} members={ctx.members} onCommit={(v) => patch(description.slug, v)} />
            </div>
          )}
          {children.length > 0 && (
            <section aria-label={t('childIssues')}>
              <div className="mb-1 flex items-center justify-between">
                <h3 className="text-base font-semibold">{t('childIssues')}</h3>
                <span className="text-xs text-muted">{t('childProgress', { done, total: children.length })}</span>
              </div>
              <div className="mb-2 h-1.5 overflow-hidden rounded bg-[#DFE1E6]">
                <div className="h-full bg-[#1F845A]" style={{ width: `${(done / children.length) * 100}%` }} />
              </div>
              <ul className="divide-y divide-[#DCDFE4] rounded-[3px] border border-[#DCDFE4]">
                {children.map((c) => {
                  const ct = issueType(c, ctx.cardTypes);
                  const st = list?.stages.find((s) => s.id === c.lists[0]?.list_stage_id);
                  return (
                    <li key={c.id} className="flex items-center gap-2 px-3 py-1.5 text-sm">
                      <IssueTypeIcon icon={ct?.icon} color={ct?.color} />
                      <span className="w-20 text-xs text-[#626F86]">{c.key}</span>
                      <Link className="min-w-0 flex-1 truncate hover:underline" href={`/workspaces/${workspaceId}/cards/${c.id}`}>
                        {issueTitle(c, ctx.cardTypes)}
                      </Link>
                      {st && list && <Lozenge kind={stageKind(st, list.stages[0]?.id === st.id)}>{st.name}</Lozenge>}
                    </li>
                  );
                })}
              </ul>
            </section>
          )}
          <IssueActivity cardId={card.id} projectId={ctx.workspace?.project_id} />
        </div>

        <aside className="flex flex-col gap-4">
          {list && stage && (
            <Dropdown>
              <Dropdown.Trigger className={buttonVariants({ variant: 'ghost', className: 'h-auto self-start p-0' })} data-testid="status-button" aria-label={t('status')}>
                <Lozenge kind={stageKind(stage, list.stages[0]?.id === stage.id)} className="gap-1 px-2 py-1 text-xs" trailing={<ChevronDown size={12} />}>
                  {stage.name}
                </Lozenge>
              </Dropdown.Trigger>
              <Dropdown.Popover>
                <Dropdown.Menu
                  aria-label={t('status')}
                  onAction={async (key) => {
                    try {
                      await putList.mutateAsync({ id: card.id, listId: list.id, data: { stage_id: String(key) } });
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
          <section className="rounded-[3px] border border-[#DCDFE4]" aria-label={t('details')}>
            <h3 className="border-b border-[#DCDFE4] px-4 py-2.5 text-sm font-semibold">{t('details')}</h3>
            <div className="flex flex-col gap-3 p-4">
              {details.map((f) => (
                <FieldEditor key={f.id} field={f} value={card.data[f.slug]} members={ctx.members} onCommit={(v) => patch(f.slug, v)} />
              ))}
              {primary && list?.methodology === 'scrum' && (
                <SimpleSelect
                  label={t('sprint')}
                  value={primary.sprint_id ?? '__backlog'}
                  onChange={async (v) => {
                    try {
                      await putCardSprint(card.id, { list_id: primary.list_id, sprint_id: !v || v === '__backlog' ? null : v });
                      await qc.invalidateQueries();
                    } catch (e) {
                      fail(e);
                    }
                  }}
                  options={[{ value: '__backlog', label: t('backlog') }, ...sprints.filter((s) => s.state !== 'completed' || s.id === primary.sprint_id).map((s) => ({ value: s.id, label: s.name }))]}
                  testId="sprint-select"
                />
              )}
            </div>
          </section>
          <p className="text-xs text-muted">
            {t('createdAt', { date: formatDate(card.created_at, locale, true) })}
            <br />
            {t('updatedAt', { date: formatDate(card.updated_at, locale, true) })}
          </p>
        </aside>
      </div>
    );
  })();

  const actions = card && (
    <div className="flex items-center gap-1">
      <Button
        size="sm"
        variant="tertiary"
        aria-pressed={watching}
        data-testid="watch"
        onPress={async () => {
          try {
            if (watching) await removeWatcher.mutateAsync({ params: { resource_type: 'card', resource_id: card.id } });
            else await addWatcher.mutateAsync({ data: { resource_type: 'card', resource_id: card.id } });
            await qc.invalidateQueries();
          } catch (e) {
            fail(e);
          }
        }}
      >
        {watching ? <EyeOff size={14} /> : <Eye size={14} />}
        {watching ? t('unwatch') : t('watch')}
      </Button>
      <Button isIconOnly size="sm" variant="ghost" aria-label={t('copyLink')} onPress={() => navigator.clipboard?.writeText(`${window.location.origin}/workspaces/${workspaceId}/cards/${card.id}`).then(() => toast.success(t('linkCopied')))}>
        <Link2 size={16} />
      </Button>
      <Dropdown>
        <Dropdown.Trigger className={buttonVariants({ variant: 'ghost', isIconOnly: true, size: 'sm' })} aria-label={t('more')} data-testid="issue-menu">
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
              <span className="flex items-center gap-2" data-testid="delete-card">
                <Trash2 size={14} />
                {t('delete')}
              </span>
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );

  const crumb = card && type && (
    <div className="flex items-center gap-2 text-sm text-[#44546F]">
      <IssueTypeIcon icon={type.icon} color={type.color} title={type.name} />
      <Link href={`/workspaces/${workspaceId}/cards/${card.id}`} className="font-medium hover:underline" data-testid="issue-key">
        {card.key}
      </Link>
    </div>
  );

  if (variant === 'page') {
    return (
      <div className="mx-auto max-w-6xl p-6">
        <div className="mb-4 flex items-center justify-between">
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

