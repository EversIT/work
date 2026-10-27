'use client';

import { routes } from '../../lib/routes';
import { SearchBox } from '../ui/inputs';
import { Button, Table } from '@heroui/react';
import { Plus } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useState } from 'react';
import { useListMyIssues } from '../../api/generated/cards/cards';
import { body } from '../../lib/api';
import { issueTitle, issueType } from '../../lib/issue';
import { IssueTypeIcon, PriorityIcon } from '../jira/issue-ui';
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
    <div className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-6">
      <h1
        className="text-2xl font-medium text-foreground"
        data-testid="workspace-title"
      >
        {ctx.workspace?.name}
      </h1>

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
                  className="flex items-center gap-3 px-3 py-2 text-sm hover:bg-surface-secondary"
                >
                  <IssueTypeIcon
                    icon={type?.icon}
                    color={type?.color}
                    title={type?.name}
                  />
                  <span className="w-20 shrink-0 text-xs font-medium text-muted">
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
          <div className="flex items-center gap-2">
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
