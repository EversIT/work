'use client';

import { routes } from '../../lib/routes';
import { SearchBox } from '../ui/inputs';
import {
  Button,
  Dropdown,
  Popover,
  Spinner,
  Switch,
  buttonVariants,
  toast,
  Label,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Filter, MoreHorizontal, Settings2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { usePathname, useRouter, useSearchParams } from 'next/navigation';
import { useEffect, useMemo, useState } from 'react';
import { usePutCardList } from '../../api/generated/cards/cards';
import {
  useCreateView,
  useDeleteView,
  useGetListBySlug,
  useUpdateView,
} from '../../api/generated/lists/lists';
import type { CardOut, SearchBody } from '../../api/generated/model';
import { useListSprints } from '../../api/generated/sprints/sprints';
import { body, errorKind, serverMessage } from '../../lib/api';
import { useCards } from '../../lib/cards';
import { rememberProject } from '../../lib/recent';
import { GROUPABLE } from '../../lib/grouping';
import { fullName } from '../../lib/format';
import { IssueView } from '../cards/issue-view';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { UserAvatar } from '../shell/user-avatar';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { BoardView } from './board-view';
import { CalendarView } from './calendar-view';
import {
  filterToRows,
  FilterBuilder,
  rowsToFilter,
  type FilterRow,
} from './filter-builder';
import { GanttView } from './gantt-view';
import { ListView } from './list-view';
import { CompleteSprintDialog } from './sprint-dialogs';
import { TableView } from './table-view';
import type { ViewOptions } from './types';

const PAGE = 50;

export function ListPage({
  workspaceId,
  slug,
}: {
  workspaceId: string;
  slug: string;
}) {
  const t = useTranslations('views');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const ctx = useWorkspaceCtx();
  const list = body(useGetListBySlug(workspaceId, slug).data);
  const scrum = list?.methodology === 'scrum';
  const sprints =
    body(
      useListSprints(list?.id ?? '', { query: { enabled: !!list && scrum } })
        .data,
    ) ?? [];
  const activeSprint = sprints.find((s) => s.state === 'active');
  const fields = useMemo(
    () =>
      ctx.cardTypes.find((c) => c.id === list?.default_card_type_id)?.fields ??
      ctx.cardTypes[0]?.fields ??
      [],
    [ctx.cardTypes, list],
  );
  const putList = usePutCardList();
  const updateView = useUpdateView();
  const createView = useCreateView();
  const deleteView = useDeleteView();

  const viewId = params.get('view');
  const view = list?.views.find((v) => v.id === viewId) ?? list?.views[0];
  const openCard = params.get('card');
  const opts = ((view?.options ?? {}) as ViewOptions) || {};
  const brief = ctx.lists.find((l) => l.slug === slug);

  const [keyword, setKeyword] = useState('');
  const [mine, setMine] = useState(false);
  const [people, setPeople] = useState<string[]>([]);
  const [typeFilter, setTypeFilter] = useState<string | null>(null);
  const [now] = useState(() => Date.now());
  // The filter rows follow the view's saved filters until the user edits them; the page number
  // starts over whenever the view, a filter or the search changes. Both are derived, not synchronised.
  const filterKey = `${view?.id}:${JSON.stringify(view?.filters ?? null)}`;
  const [editedRows, setEditedRows] = useState<{
    key: string;
    rows: FilterRow[];
  } | null>(null);
  const rows =
    editedRows?.key === filterKey
      ? editedRows.rows
      : filterToRows(view?.filters);
  const setRows = (next: FilterRow[]) =>
    setEditedRows({ key: filterKey, rows: next });
  const pageKey = [
    view?.id,
    keyword,
    JSON.stringify(rows),
    mine,
    people.join(','),
    typeFilter,
  ].join('|');
  const [pageState, setPageState] = useState({ key: pageKey, page: 1 });
  const page = pageState.key === pageKey ? pageState.page : 1;
  const setPage = (n: number) => setPageState({ key: pageKey, page: n });
  const [newView, setNewView] = useState(false);
  const [newViewType, setNewViewType] = useState<string | null>('board');
  const [completing, setCompleting] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem('work.lastWorkspace', workspaceId);
      if (list) localStorage.setItem('work.lastList', list.id);
      if (list) rememberProject(workspaceId, list.slug);
    } catch {
      /* ignore */
    }
  }, [workspaceId, list]);

  const paged = view?.type === 'table';
  const sprintScoped =
    scrum && (view?.type === 'board' || view?.type === 'list');
  const assigneeSlug = fields.find((f) => f.is_assignee)?.slug;
  const who = [...people, ...(mine && ctx.me ? [ctx.me.id] : [])];

  const tree = rowsToFilter(rows, fields);
  const extra: unknown[] = [];
  if (who.length && assigneeSlug)
    extra.push({ field: `data.${assigneeSlug}`, operator: 'in', value: who });
  if (typeFilter)
    extra.push({ field: 'card_type_id', operator: 'eq', value: typeFilter });
  const and = [...(tree?.and ?? []), ...extra];

  const search: SearchBody = {
    page: paged ? page : 1,
    limit: paged ? PAGE : 200,
    hide_completed: opts.hideCompleted ?? false,
    hide_children: opts.hideChildren ?? false,
    keyword: keyword || undefined,
    sort_by: opts.sortBy ?? 'order',
    sort_order: opts.sortOrder ?? 'asc',
    filters: and.length ? { and } : undefined,
    sprint: sprintScoped ? 'active' : undefined,
  };
  const cards = useCards(list?.id, search);

  const setParam = (k: string, v: string | null) => {
    const p = new URLSearchParams(params.toString());
    if (v) p.set(k, v);
    else p.delete(k);
    router.replace(`${pathname}?${p.toString()}`);
  };
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));

  if (!list || !view)
    return (
      <div className="flex justify-center p-12">
        <Spinner aria-label={t('loading')} />
      </div>
    );

  const saveOptions = async (patch: Partial<ViewOptions>) => {
    try {
      await updateView.mutateAsync({
        id: view.id,
        data: { options: { ...opts, ...patch } },
      });
      await qc.invalidateQueries();
    } catch (e) {
      fail(e);
    }
  };
  const saveFilters = async (next: FilterRow[]) => {
    setRows(next);
    const f = rowsToFilter(next, fields);
    if (JSON.stringify(f) === JSON.stringify(view.filters ?? null)) return;
    try {
      await updateView.mutateAsync({
        id: view.id,
        data: { filters: f ?? { and: [] } },
      });
    } catch (e) {
      fail(e);
    }
  };

  const viewProps = {
    list,
    stages: list.stages,
    fields,
    cards: cards.data?.cards ?? [],
    members: ctx.members,
    view,
    activeSprintId: activeSprint?.id,
    onOpen: (id: string) => setParam('card', id),
    onMove: async (card: CardOut, stageId: string) => {
      try {
        await putList.mutateAsync({
          id: card.id,
          listId: list.id,
          data: { stage_id: stageId },
        });
        await qc.invalidateQueries(); // cards, sprint counters and the issue view all change
      } catch (e) {
        fail(e);
      }
    },
    onQuickCreate: async () => undefined,
  };
  const total = cards.data?.total ?? 0;
  const daysLeft = activeSprint?.ends_at
    ? Math.max(
        0,
        Math.ceil(
          (new Date(activeSprint.ends_at).getTime() - now) / 86_400_000,
        ),
      )
    : null;

  return (
    <div className="flex flex-col gap-3 px-6 py-4">
      <nav aria-label="breadcrumb" className="text-xs text-muted">
        <Link
          href={routes.workspace(ctx.workspace?.key_prefix ?? '')}
          className="hover:underline"
        >
          {t('projects')}
        </Link>{' '}
        / <span>{brief?.spaceName}</span> / <span>{list.name}</span>
      </nav>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1
          className="text-2xl font-medium text-foreground"
          data-testid="list-title"
        >
          {view.name === 'Board' || !view.name ? list.name : `${list.name}`}
        </h1>
        <span className="text-sm text-muted" data-testid="view-name">
          {view.name}
        </span>
      </div>

      {scrum && sprintScoped && (
        <div
          className="flex flex-wrap items-center justify-between gap-2 rounded-xl bg-surface-secondary px-3 py-2"
          data-testid="sprint-banner"
        >
          {activeSprint ? (
            <>
              <div>
                <span className="font-semibold">{activeSprint.name}</span>
                {daysLeft !== null && (
                  <span className="ml-3 text-xs text-muted">
                    {t('daysLeft', { count: daysLeft })}
                  </span>
                )}
                {activeSprint.goal && (
                  <p className="text-xs text-muted">
                    {t('goal')}: {activeSprint.goal}
                  </p>
                )}
              </div>
              <Button
                size="sm"
                variant="secondary"
                onPress={() => setCompleting(true)}
                data-testid="complete-sprint"
              >
                {t('completeSprint')}
              </Button>
            </>
          ) : (
            <p className="text-sm text-muted">
              {t('noActiveSprint')}{' '}
              <Link
                className="text-accent underline"
                href={routes.backlog(ctx.workspace?.key_prefix ?? '', slug)}
              >
                {t('goToBacklog')}
              </Link>
            </p>
          )}
        </div>
      )}

      <div
        className="flex flex-wrap items-center gap-2"
        data-testid="quick-filters"
      >
        <SearchBox
          label={t('search')}
          maxLength={200}
          value={keyword}
          onChange={setKeyword}
          testId="search"
        />
        <fieldset className="m-0 flex min-w-0 -space-x-1 border-0 p-0">
          <legend className="sr-only">{t('assignee')}</legend>
          {ctx.members.slice(0, 6).map((m) => (
            <Button
              isIconOnly
              key={m.user_id}
              aria-label={fullName(m)}
              aria-pressed={people.includes(m.user_id)}
              onPress={() =>
                setPeople((p) =>
                  p.includes(m.user_id)
                    ? p.filter((x) => x !== m.user_id)
                    : [...p, m.user_id],
                )
              }
              variant="ghost"
              size="sm"
              className={`size-auto min-w-0 rounded-full border-2 p-0 ${people.includes(m.user_id) ? 'border-accent' : 'border-background'}`}
            >
              <UserAvatar user={m} />
            </Button>
          ))}
        </fieldset>
        <Button
          size="sm"
          variant={mine ? 'primary' : 'tertiary'}
          aria-pressed={mine}
          onPress={() => setMine(!mine)}
          data-testid="only-mine"
        >
          {t('onlyMine')}
        </Button>
        <div className="w-36">
          <SimpleSelect
            label={t('type.label')}
            value={typeFilter ?? '__all'}
            onChange={(v) => setTypeFilter(!v || v === '__all' ? null : v)}
            options={[
              { value: '__all', label: t('allTypes') },
              ...ctx.cardTypes.map((c) => ({ value: c.id, label: c.name })),
            ]}
            testId="type-filter"
          />
        </div>
        <Popover>
          <Button size="sm" variant="tertiary" data-testid="filters">
            <Filter size={14} />
            {t('filters')}
            {rows.length > 0 ? ` (${rows.length})` : ''}
          </Button>
          <Popover.Content placement="bottom start">
            <Popover.Dialog>
              <FilterBuilder
                rows={rows}
                onChange={saveFilters}
                fields={fields}
                stages={list.stages}
                members={ctx.members}
              />
            </Popover.Dialog>
          </Popover.Content>
        </Popover>
        <Popover>
          <Button size="sm" variant="tertiary" aria-label={t('viewSettings')}>
            <Settings2 size={14} />
          </Button>
          <Popover.Content placement="bottom end">
            <Popover.Dialog>
              <div className="flex w-64 flex-col gap-3 p-3">
                <SimpleSelect
                  label={t('sortBy')}
                  value={opts.sortBy ?? 'order'}
                  onChange={(v) => v && saveOptions({ sortBy: v })}
                  options={[
                    { value: 'order', label: t('manualOrder') },
                    { value: 'created_at', label: t('created') },
                    { value: 'updated_at', label: t('updated') },
                    ...fields
                      .filter(
                        (f) =>
                          ![
                            'rich',
                            'user',
                            'card',
                            'stage',
                            'dropdown',
                            'label',
                          ].includes(f.type),
                      )
                      .map((f) => ({ value: `data.${f.slug}`, label: f.name })),
                  ]}
                />
                {(view.type === 'table' || view.type === 'list') && (
                  <SimpleSelect
                    label={t('groupBy')}
                    value={
                      opts.groupBy?.type === 'list_stage'
                        ? '__stage'
                        : opts.groupBy?.type === 'field'
                          ? `field:${opts.groupBy.fieldId}`
                          : '__none'
                    }
                    testId="group-by"
                    onChange={(v) => {
                      if (!v) return;
                      saveOptions({
                        groupBy:
                          v === '__none'
                            ? undefined
                            : v === '__stage'
                              ? { type: 'list_stage' }
                              : { type: 'field', fieldId: v.slice(6) },
                      });
                    }}
                    options={[
                      { value: '__none', label: t('noGrouping') },
                      { value: '__stage', label: t('status') },
                      ...fields
                        .filter((f) => GROUPABLE.includes(f.type))
                        .map((f) => ({
                          value: `field:${f.id}`,
                          label: f.name,
                        })),
                    ]}
                  />
                )}
                <Switch
                  className="flex flex-row items-center gap-2"
                  isSelected={opts.hideCompleted ?? false}
                  onChange={(v) => saveOptions({ hideCompleted: v })}
                >
                  <Switch.Control>
                    <Switch.Thumb />
                  </Switch.Control>
                  <Switch.Content>
                    <Label>{t('hideCompleted')}</Label>
                  </Switch.Content>
                </Switch>
                <Switch
                  className="flex flex-row items-center gap-2"
                  isSelected={opts.hideChildren ?? false}
                  onChange={(v) => saveOptions({ hideChildren: v })}
                >
                  <Switch.Control>
                    <Switch.Thumb />
                  </Switch.Control>
                  <Switch.Content>
                    <Label>{t('hideChildren')}</Label>
                  </Switch.Content>
                </Switch>
              </div>
            </Popover.Dialog>
          </Popover.Content>
        </Popover>
        <Dropdown>
          <Dropdown.Trigger
            className={buttonVariants({
              variant: 'ghost',
              isIconOnly: true,
              size: 'sm',
            })}
            aria-label={t('viewMenu')}
            data-testid="view-menu"
          >
            <MoreHorizontal size={14} />
          </Dropdown.Trigger>
          <Dropdown.Popover>
            <Dropdown.Menu
              aria-label={t('viewMenu')}
              onAction={async (key) => {
                if (key === 'new') setNewView(true);
                if (key === 'delete') {
                  if (list.views.length <= 1) {
                    toast.danger(t('lastView'));
                    return;
                  }
                  if (!window.confirm(t('confirmDeleteView'))) return;
                  try {
                    await deleteView.mutateAsync({ id: view.id });
                    setParam('view', null);
                    await qc.invalidateQueries();
                  } catch (e) {
                    fail(e);
                  }
                }
              }}
            >
              <Dropdown.Item id="new" textValue={t('newView')}>
                {t('newView')}
              </Dropdown.Item>
              <Dropdown.Item
                id="delete"
                textValue={t('deleteView')}
                variant="danger"
              >
                {t('deleteView')}
              </Dropdown.Item>
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
      </div>

      {cards.isLoading ? (
        <div className="flex justify-center p-8">
          <Spinner aria-label={t('loading')} />
        </div>
      ) : (
        <>
          {view.type === 'table' && <TableView {...viewProps} />}
          {view.type === 'board' && <BoardView {...viewProps} />}
          {view.type === 'list' && <ListView {...viewProps} />}
          {view.type === 'calendar' && <CalendarView {...viewProps} />}
          {view.type === 'gantt' && <GanttView {...viewProps} />}
          {paged && total > PAGE && (
            <div className="flex items-center justify-center gap-3 text-sm">
              <Button
                size="sm"
                variant="tertiary"
                isDisabled={page <= 1}
                onPress={() => setPage(page - 1)}
              >
                {t('previous')}
              </Button>
              <span>
                {t('pageOf', { page, pages: Math.ceil(total / PAGE) })}
              </span>
              <Button
                size="sm"
                variant="tertiary"
                isDisabled={page * PAGE >= total}
                onPress={() => setPage(page + 1)}
              >
                {t('next')}
              </Button>
            </div>
          )}
          {!paged && total > 200 && (
            <p className="text-xs text-muted">
              {t('truncated', { count: 200, total })}
            </p>
          )}
        </>
      )}

      {newView && (
        <FormDialog
          isOpen
          onClose={() => setNewView(false)}
          title={t('newView')}
          submitLabel={t('create')}
          onSubmit={async (f) => {
            try {
              const res = await createView.mutateAsync({
                id: list.id,
                data: {
                  name: String(f.get('name')),
                  type: newViewType ?? 'board',
                  options: {},
                },
              });
              await qc.invalidateQueries();
              setNewView(false);
              if (res.status === 200) setParam('view', res.data.id);
            } catch (e) {
              fail(e);
            }
          }}
        >
          <TextInput
            label={t('viewName')}
            name="name"
            required
            maxLength={100}
          />
          <SimpleSelect
            label={t('viewType')}
            value={newViewType}
            onChange={setNewViewType}
            options={['board', 'table', 'list', 'calendar', 'gantt'].map(
              (v) => ({ value: v, label: t(`type.${v}`) }),
            )}
          />
        </FormDialog>
      )}
      {completing && activeSprint && (
        <CompleteSprintDialog
          sprint={activeSprint}
          planned={sprints.filter((s) => s.state === 'planned')}
          onClose={() => setCompleting(false)}
        />
      )}
      {openCard && (
        <IssueView
          cardId={openCard}
          workspaceId={workspaceId}
          variant="modal"
          onClose={() => setParam('card', null)}
        />
      )}
    </div>
  );
}
