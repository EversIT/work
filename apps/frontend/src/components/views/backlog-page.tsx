'use client';

import { routes } from '../../lib/routes';
import {
  Button,
  Dropdown,
  Spinner,
  buttonVariants,
  toast,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, MoreHorizontal, Plus } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import Link from 'next/link';
import { usePathname, useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { putCardList, putCardSprint } from '../../api/generated/cards/cards';
import { BacklogAssistant } from './ai-reports';
import { useGetListBySlug } from '../../api/generated/lists/lists';
import type { CardOut, SprintOut } from '../../api/generated/model';
import {
  useCreateSprint,
  useDeleteSprint,
  useListSprints,
} from '../../api/generated/sprints/sprints';
import { body, errorKind, serverMessage } from '../../lib/api';
import { useCards } from '../../lib/cards';
import { formatDate } from '../../lib/format';
import { issueAssignees, issueTitle, issueType } from '../../lib/issue';
import { IssueView } from '../cards/issue-view';
import {
  IssueTypeIcon,
  Lozenge,
  PointsBadge,
  PriorityIcon,
  stageKind,
} from '../jira/issue-ui';
import { useCreateIssue } from '../shell/create-issue-context';
import { UserAvatar } from '../shell/user-avatar';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { CompleteSprintDialog, StartSprintDialog } from './sprint-dialogs';

type Section = { id: string | null; sprint?: SprintOut; cards: CardOut[] };

/** Scrum backlog: planned/active sprints and the backlog, with drag and drop between them. */
export function BacklogPage({
  workspaceId,
  slug,
}: {
  workspaceId: string;
  slug: string;
}) {
  const t = useTranslations('backlog');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const ctx = useWorkspaceCtx();
  const create = useCreateIssue();
  const list = body(useGetListBySlug(workspaceId, slug).data);
  const sprints =
    body(useListSprints(list?.id ?? '', { query: { enabled: !!list } }).data) ??
    [];
  const cards = useCards(list?.id, { limit: 200, sort_by: 'order' });
  const createSprint = useCreateSprint();
  const delSprint = useDeleteSprint();
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [over, setOver] = useState<string | null>(null);
  // The row a dragged issue would land next to, and on which side of it.
  const [target, setTarget] = useState<{ id: string; after: boolean } | null>(
    null,
  );
  const [starting, setStarting] = useState<SprintOut | null>(null);
  const [completing, setCompleting] = useState<SprintOut | null>(null);
  const openCard = params.get('card');
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));

  if (!list)
    return (
      <div className="flex justify-center p-12">
        <Spinner aria-label={t('loading')} />
      </div>
    );
  if (list.methodology !== 'scrum') {
    return <p className="p-6 text-muted">{t('notScrum')}</p>;
  }

  const live = sprints.filter((s) => s.state !== 'completed');
  const all = cards.data?.cards ?? [];
  const sprintOf = (c: CardOut) =>
    c.lists.find((l) => l.list_id === list.id)?.sprint_id ?? null;
  const visibleSprintIds = new Set(live.map((s) => s.id));
  const sections: Section[] = [
    ...live.map((s) => ({
      id: s.id,
      sprint: s,
      cards: all.filter((c) => sprintOf(c) === s.id),
    })),
    {
      id: null,
      cards: all.filter((c) => {
        const sp = sprintOf(c);
        return sp === null || !visibleSprintIds.has(sp) ? sp === null : false;
      }),
    },
  ];
  const hasActive = live.some((s) => s.state === 'active');
  const open = (id: string | null) => {
    const p = new URLSearchParams(params.toString());
    if (id) p.set('card', id);
    else p.delete('card');
    router.replace(`${pathname}?${p.toString()}`);
  };

  const drop = async (card: CardOut, sprintId: string | null) => {
    if (sprintOf(card) === sprintId) return;
    try {
      await putCardSprint(card.id, { list_id: list.id, sprint_id: sprintId });
      await qc.invalidateQueries();
    } catch (e) {
      fail(e);
    }
  };

  const orderOf = (c: CardOut) =>
    c.lists.find((l) => l.list_id === list.id)?.sort_order ?? 0;
  const stageIdOf = (c: CardOut) =>
    c.lists.find((l) => l.list_id === list.id)?.list_stage_id ?? '';

  /**
   * Puts `card` into `sec` next to `ref` (before it, or after it), or at the end when there is no
   * `ref`. Order is a number per issue: the new one goes halfway between its new neighbours, and
   * when there is no room left between them the whole section is numbered afresh.
   */
  const place = async (
    card: CardOut,
    sec: Section,
    ref: CardOut | null,
    after: boolean,
  ) => {
    const rest = sec.cards.filter((c) => c.id !== card.id);
    let at = ref
      ? rest.findIndex((c) => c.id === ref.id) + (after ? 1 : 0)
      : rest.length;
    if (at < 0) at = rest.length;
    const prev = rest[at - 1];
    const next = rest[at];
    try {
      if (sprintOf(card) !== sec.id)
        await putCardSprint(card.id, { list_id: list.id, sprint_id: sec.id });
      const lo = prev ? orderOf(prev) : next ? orderOf(next) - 200 : 0;
      const hi = next ? orderOf(next) : lo + 200;
      if (hi - lo >= 2) {
        await putCardList(card.id, list.id, {
          stage_id: stageIdOf(card),
          order: Math.floor((lo + hi) / 2),
        });
      } else {
        const next_order = [...rest.slice(0, at), card, ...rest.slice(at)];
        await Promise.all(
          next_order.map((c, i) =>
            (i + 1) * 100 === orderOf(c)
              ? undefined
              : putCardList(c.id, list.id, {
                  stage_id: stageIdOf(c),
                  order: (i + 1) * 100,
                }),
          ),
        );
      }
      await qc.invalidateQueries();
    } catch (e) {
      fail(e);
    }
  };

  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      <nav aria-label="breadcrumb" className="text-xs text-muted">
        <Link
          href={routes.workspace(ctx.workspace?.key_prefix ?? '')}
          className="hover:underline"
        >
          {t('projects')}
        </Link>{' '}
        / <span>{list.name}</span>
      </nav>
      <div className="flex items-center justify-between">
        <h1
          className="text-2xl font-medium text-foreground"
          data-testid="backlog-title"
        >
          {t('title')}
        </h1>
        <div className="flex flex-wrap items-center gap-2">
          <BacklogAssistant
            listId={list.id}
            planned={live.filter((s) => s.state === 'planned')}
          />
          <Button
            size="sm"
            variant="secondary"
            isPending={createSprint.isPending}
            data-testid="create-sprint"
            onPress={async () => {
              try {
                await createSprint.mutateAsync({ id: list.id, data: {} });
                await qc.invalidateQueries();
              } catch (e) {
                fail(e);
              }
            }}
          >
            <Plus size={14} />
            {t('createSprint')}
          </Button>
        </div>
      </div>

      {sections.map((sec) => {
        const key = sec.id ?? 'backlog';
        const isOpen = !collapsed[key];
        const stageOfCard = (c: CardOut) =>
          list.stages.find(
            (s) =>
              s.id ===
              c.lists.find((l) => l.list_id === list.id)?.list_stage_id,
          );
        const counts = { todo: 0, progress: 0, done: 0 };
        for (const c of sec.cards) {
          const st = stageOfCard(c);
          if (!st) continue;
          counts[stageKind(st, list.stages[0]?.id === st.id)] += 1;
        }
        const points = sec.cards.reduce(
          (n, c) =>
            n +
            (typeof c.data.story_points === 'number' ? c.data.story_points : 0),
          0,
        );
        return (
          // A drop target for dragged issues: the drag events cannot sit on an inner element.
          // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
          <section
            key={key}
            aria-label={sec.sprint?.name ?? t('backlog')}
            data-testid={
              sec.sprint ? `sprint-${sec.sprint.name}` : 'backlog-section'
            }
            className={`rounded-2xl bg-surface-secondary p-2 ${over === key ? 'ring-2 ring-accent' : ''}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(key);
            }}
            onDragLeave={() => setOver((o) => (o === key ? null : o))}
            onDrop={(e) => {
              e.preventDefault();
              setOver(null);
              const card = all.find(
                (c) => c.id === e.dataTransfer.getData('text/card-id'),
              );
              if (card) void drop(card, sec.id);
            }}
          >
            <header className="flex flex-wrap items-center gap-2 px-1 py-1">
              <Button
                aria-expanded={isOpen}
                onPress={() => setCollapsed((c) => ({ ...c, [key]: isOpen }))}
                variant="ghost"
                size="sm"
                className="px-1 font-semibold"
              >
                {isOpen ? (
                  <ChevronDown size={16} />
                ) : (
                  <ChevronRight size={16} />
                )}
                {sec.sprint?.name ?? t('backlog')}
              </Button>
              {sec.sprint?.state === 'active' && sec.sprint.ends_at && (
                <span className="text-xs text-muted">
                  {formatDate(sec.sprint.starts_at, locale)} –{' '}
                  {formatDate(sec.sprint.ends_at, locale)}
                </span>
              )}
              <span className="text-xs text-muted">
                ({t('issues', { count: sec.cards.length })})
              </span>
              <span className="ml-auto flex items-center gap-1.5">
                <Lozenge kind="todo">{counts.todo}</Lozenge>
                <Lozenge kind="progress">{counts.progress}</Lozenge>
                <Lozenge kind="done">{counts.done}</Lozenge>
                <PointsBadge value={points} />
                {sec.sprint?.state === 'planned' && (
                  <Button
                    size="sm"
                    variant="secondary"
                    isDisabled={hasActive}
                    onPress={() => setStarting(sec.sprint ?? null)}
                    data-testid={`start-${sec.sprint.name}`}
                  >
                    {t('start')}
                  </Button>
                )}
                {sec.sprint?.state === 'active' && (
                  <Button
                    size="sm"
                    variant="secondary"
                    onPress={() => setCompleting(sec.sprint ?? null)}
                    data-testid="complete-sprint"
                  >
                    {t('complete')}
                  </Button>
                )}
                {sec.sprint?.state === 'planned' && (
                  <Dropdown>
                    <Dropdown.Trigger
                      className={buttonVariants({
                        variant: 'ghost',
                        isIconOnly: true,
                        size: 'sm',
                      })}
                      aria-label={t('sprintMenu')}
                    >
                      <MoreHorizontal size={14} />
                    </Dropdown.Trigger>
                    <Dropdown.Popover>
                      <Dropdown.Menu
                        aria-label={t('sprintMenu')}
                        onAction={async () => {
                          if (!window.confirm(t('confirmDeleteSprint'))) return;
                          try {
                            await delSprint.mutateAsync({
                              id: sec.sprint?.id ?? '',
                            });
                            await qc.invalidateQueries();
                          } catch (e) {
                            fail(e);
                          }
                        }}
                      >
                        <Dropdown.Item
                          id="delete"
                          textValue={t('deleteSprint')}
                          variant="danger"
                        >
                          {t('deleteSprint')}
                        </Dropdown.Item>
                      </Dropdown.Menu>
                    </Dropdown.Popover>
                  </Dropdown>
                )}
              </span>
            </header>
            {sec.sprint?.goal && (
              <p className="px-2 pb-1 text-xs text-muted">
                {t('goal')}: {sec.sprint.goal}
              </p>
            )}
            {isOpen && (
              <>
                <ul
                  className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-surface"
                  data-testid={`rows-${sec.sprint?.name ?? 'backlog'}`}
                >
                  {sec.cards.length === 0 && (
                    <li className="px-3 py-4 text-center text-sm text-muted">
                      {sec.sprint ? t('emptySprint') : t('emptyBacklog')}
                    </li>
                  )}
                  {sec.cards.map((card) => {
                    const type = issueType(card, ctx.cardTypes);
                    const st = stageOfCard(card);
                    const people = issueAssignees(
                      card,
                      ctx.cardTypes,
                      ctx.members,
                    );
                    return (
                      // A drop target for dragged issues: the events cannot sit on an inner element.
                      // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
                      <li
                        key={card.id}
                        style={
                          target?.id === card.id
                            ? {
                                boxShadow: target.after
                                  ? 'inset 0 -2px 0 0 var(--accent)'
                                  : 'inset 0 2px 0 0 var(--accent)',
                              }
                            : undefined
                        }
                        className="transition-shadow"
                        onDragOver={(e) => {
                          e.preventDefault();
                          const r = e.currentTarget.getBoundingClientRect();
                          const after = e.clientY > r.top + r.height / 2;
                          setTarget((c) =>
                            c?.id === card.id && c.after === after
                              ? c
                              : { id: card.id, after },
                          );
                        }}
                        onDrop={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          const dragged = all.find(
                            (c) =>
                              c.id === e.dataTransfer.getData('text/card-id'),
                          );
                          const after = target?.after ?? false;
                          setTarget(null);
                          setOver(null);
                          if (dragged && dragged.id !== card.id)
                            void place(dragged, sec, card, after);
                        }}
                      >
                        <div
                          draggable
                          onDragStart={(e) =>
                            e.dataTransfer.setData('text/card-id', card.id)
                          }
                          onDragEnd={() => setTarget(null)}
                          data-testid={`row-${issueTitle(card, ctx.cardTypes)}`}
                          className="flex cursor-grab items-center gap-3 px-3 py-1.5 text-sm hover:bg-surface-secondary"
                        >
                          <IssueTypeIcon
                            icon={type?.icon}
                            color={type?.color}
                            title={type?.name}
                          />
                          <span className="w-20 shrink-0 text-xs font-medium text-muted">
                            {card.key}
                          </span>
                          {/* A native button: HeroUI's press handling would swallow the row's drag. */}
                          <button
                            type="button"
                            className="min-w-0 flex-1 truncate text-left hover:text-accent hover:underline"
                            onClick={() => open(card.id)}
                            onKeyDown={(e) => {
                              // Alt + arrow keys move the issue, for those who do not drag.
                              if (!e.altKey) return;
                              const i = sec.cards.findIndex(
                                (c) => c.id === card.id,
                              );
                              if (e.key === 'ArrowUp' && i > 0) {
                                e.preventDefault();
                                void place(card, sec, sec.cards[i - 1], false);
                              }
                              if (
                                e.key === 'ArrowDown' &&
                                i < sec.cards.length - 1
                              ) {
                                e.preventDefault();
                                void place(card, sec, sec.cards[i + 1], true);
                              }
                            }}
                          >
                            {issueTitle(card, ctx.cardTypes)}
                          </button>
                          {st && (
                            <Lozenge
                              kind={stageKind(st, list.stages[0]?.id === st.id)}
                            >
                              {st.name}
                            </Lozenge>
                          )}
                          <PriorityIcon value={card.data.priority} />
                          <PointsBadge value={card.data.story_points} />
                          <span className="w-6">
                            {people[0] && (
                              <UserAvatar user={people[0]} size="sm" />
                            )}
                          </span>
                        </div>
                      </li>
                    );
                  })}
                </ul>
                <Button
                  onPress={() =>
                    create.open({ listId: list.id, sprintId: sec.id })
                  }
                  data-testid={`create-in-${sec.sprint?.name ?? 'backlog'}`}
                  variant="ghost"
                  size="sm"
                  className="mt-1 justify-start text-muted"
                >
                  <Plus size={14} />
                  {t('createIssue')}
                </Button>
              </>
            )}
          </section>
        );
      })}
      {starting && (
        <StartSprintDialog
          sprint={starting}
          onClose={() => setStarting(null)}
        />
      )}
      {completing && (
        <CompleteSprintDialog
          sprint={completing}
          planned={live.filter((s) => s.state === 'planned')}
          onClose={() => setCompleting(null)}
        />
      )}
      {openCard && (
        <IssueView
          cardId={openCard}
          workspaceId={workspaceId}
          variant="modal"
          onClose={() => open(null)}
        />
      )}
    </div>
  );
}
