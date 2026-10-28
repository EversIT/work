'use client';

import { routes } from '../../lib/routes';
import { SearchBox } from '../ui/inputs';
import { Button, Table } from '@heroui/react';
import { Plus } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useMemo, useState } from 'react';
import { useListMyIssues } from '../../api/generated/cards/cards';
import { body } from '../../lib/api';
import { issueTitle, issueType } from '../../lib/issue';
import { useIsPhone } from '../../lib/media';
import { readRecentProjects } from '../../lib/recent';
import { IssueTypeIcon, PriorityIcon } from '../jira/issue-ui';
import { CatchMeUp } from './catch-me-up';
import { useCreateProject } from './create-project-context';
import { useWorkspaceCtx } from './workspace-context';

/** "Your work": issues assigned to you, and the projects of the workspace. */
export function WorkspaceHome({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('workspace');
  const ctx = useWorkspaceCtx();
  const mine = body(useListMyIssues(workspaceId).data) ?? [];
  const router = useRouter();
  const createProject = useCreateProject();
  const [query, setQuery] = useState('');
  const phone = useIsPhone();
  const wsKey = ctx.workspace?.key_prefix ?? '';
  // Jira's "Recommended spaces": the projects opened last, then the rest.
  const recommended = useMemo(() => {
    const seen = readRecentProjects(workspaceId);
    return [
      ...seen
        .map((slug) => ctx.lists.find((l) => l.slug === slug))
        .filter((l) => !!l),
      ...ctx.lists.filter((l) => !seen.includes(l.slug)),
    ]
      .filter((l, i, all) => all.indexOf(l) === i)
      .slice(0, 6);
  }, [ctx.lists, workspaceId]);
  const shown = ctx.lists.filter((l) =>
    `${l.name} ${l.spaceName}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );

  useEffect(() => {
    try {
      localStorage.setItem('work.lastWorkspace', workspaceId);
    } catch {
      /* ignore */
    }
  }, [workspaceId]);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-8 px-4 py-6 md:px-6">
      <header>
        <h1
          className="text-2xl font-medium text-foreground"
          data-testid="workspace-title"
        >
          {ctx.workspace?.name}
        </h1>
        <p className="mt-1 text-sm text-muted">{t('forYou')}</p>
        <CatchMeUp workspaceId={workspaceId} />
      </header>

      {recommended.length > 0 && (
        <section aria-label={t('recommended')}>
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-base font-semibold text-foreground">
              {t('recommended')}
            </h2>
          </div>
          {/* A row you swipe on phones, a grid on wide screens. */}
          <ul
            className="-mx-4 flex snap-x snap-mandatory gap-3 overflow-x-auto px-4 pb-2 md:mx-0 md:grid md:grid-cols-3 md:overflow-visible md:px-0"
            data-testid="recommended-projects"
          >
            {recommended.map((l) => (
              <li key={l.id} className="w-56 shrink-0 snap-start md:w-auto">
                <Link
                  href={routes.project(wsKey, l.slug)}
                  className="flex h-full items-center gap-3 rounded-xl border border-border bg-surface p-3 transition-colors hover:bg-surface-secondary"
                >
                  <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-accent-hover to-accent text-sm font-bold text-white">
                    {l.name.slice(0, 2).toUpperCase()}
                  </span>
                  <span className="flex min-w-0 flex-col">
                    <span className="truncate text-sm font-medium text-foreground">
                      {l.name}
                    </span>
                    <span className="truncate text-xs text-muted">
                      {l.spaceName} ·{' '}
                      {l.methodology === 'scrum' ? t('scrum') : t('kanban')}
                    </span>
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-label={t('assignedToMe')}>
        <h2 className="mb-2 text-base font-semibold text-foreground">
          {t('assignedToMe')}
        </h2>
        {mine.length === 0 ? (
          <p className="text-muted" data-testid="no-assigned">
            {t('noAssigned')}
          </p>
        ) : (
          <ul
            className="divide-y divide-border overflow-hidden rounded-xl border border-border"
            data-testid="assigned-issues"
          >
            {mine.map((c) => {
              const type = issueType(c, ctx.cardTypes);
              return (
                <li
                  key={c.id}
                  className="flex min-h-11 items-center gap-3 px-3 py-2 text-sm hover:bg-surface-secondary"
                >
                  <IssueTypeIcon
                    icon={type?.icon}
                    color={type?.color}
                    title={type?.name}
                  />
                  <span className="w-16 shrink-0 text-xs font-medium text-muted md:w-20">
                    {c.key}
                  </span>
                  <Link
                    className="min-w-0 flex-1 truncate hover:text-accent hover:underline"
                    href={routes.browse(c.key)}
                  >
                    {issueTitle(c, ctx.cardTypes)}
                  </Link>
                  <PriorityIcon value={c.data.priority} />
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-label={t('projectsTitle')}>
        <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-base font-semibold text-foreground">
            {t('projectsTitle')}
          </h2>
          <div className="flex w-full items-center gap-2 sm:w-auto">
            {ctx.lists.length > 0 && (
              <SearchBox
                label={t('searchProjects')}
                value={query}
                onChange={setQuery}
                testId="project-search"
              />
            )}
            <Button
              size="sm"
              onPress={createProject.open}
              data-testid="create-project"
            >
              <Plus size={14} />
              {t('createProject')}
            </Button>
          </div>
        </div>
        {ctx.lists.length === 0 ? (
          <div
            className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-border px-6 py-12 text-center"
            data-testid="no-projects"
          >
            <p className="text-base font-semibold">{t('emptyTitle')}</p>
            <p className="max-w-md text-sm text-muted">{t('emptyText')}</p>
            <Button
              className="mt-2"
              onPress={createProject.open}
              data-testid="create-first-project"
            >
              {t('createProject')}
            </Button>
          </div>
        ) : phone ? (
          <ul
            className="divide-y divide-border overflow-hidden rounded-xl border border-border"
            data-testid="project-cards"
          >
            {shown.length === 0 && (
              <li className="p-6 text-center text-muted">{t('noMatch')}</li>
            )}
            {shown.map((l) => (
              <li key={l.id}>
                <Link
                  href={routes.project(wsKey, l.slug)}
                  className="flex min-h-14 items-center gap-3 px-3 py-2 hover:bg-surface-secondary"
                >
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-accent-hover to-accent text-xs font-bold text-white">
                    {l.name.slice(0, 2).toUpperCase()}
                  </span>
                  <span className="flex min-w-0 flex-col">
                    <span className="truncate text-sm font-medium text-accent">
                      {l.name}
                    </span>
                    <span className="truncate text-xs text-muted">
                      {l.spaceName} ·{' '}
                      {l.methodology === 'scrum' ? t('scrum') : t('kanban')} ·{' '}
                      {l.access_type === 'private' ? t('private') : t('public')}
                    </span>
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        ) : (
          <Table>
            <Table.ScrollContainer>
              <Table.Content
                aria-label={t('projectsTitle')}
                onRowAction={(key) =>
                  router.push(
                    routes.project(
                      ctx.workspace?.key_prefix ?? '',
                      String(key),
                    ),
                  )
                }
                data-testid="project-table"
              >
                <Table.Header>
                  <Table.Column isRowHeader id="name">
                    {t('colName')}
                  </Table.Column>
                  <Table.Column id="space">{t('colSpace')}</Table.Column>
                  <Table.Column id="type">{t('colType')}</Table.Column>
                  <Table.Column id="access">{t('colAccess')}</Table.Column>
                </Table.Header>
                <Table.Body
                  items={shown}
                  renderEmptyState={() => (
                    <div className="p-6 text-center text-muted">
                      {t('noMatch')}
                    </div>
                  )}
                >
                  {(l) => (
                    <Table.Row id={l.slug}>
                      <Table.Cell>
                        <span className="flex items-center gap-2.5">
                          <span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-accent-hover to-accent text-xs font-bold text-white">
                            {l.name.slice(0, 2).toUpperCase()}
                          </span>
                          <span className="font-medium text-accent">
                            {l.name}
                          </span>
                        </span>
                      </Table.Cell>
                      <Table.Cell>{l.spaceName}</Table.Cell>
                      <Table.Cell>
                        {l.methodology === 'scrum' ? t('scrum') : t('kanban')}
                      </Table.Cell>
                      <Table.Cell>
                        {l.access_type === 'private'
                          ? t('private')
                          : t('public')}
                      </Table.Cell>
                    </Table.Row>
                  )}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
        )}
      </section>
    </div>
  );
}
