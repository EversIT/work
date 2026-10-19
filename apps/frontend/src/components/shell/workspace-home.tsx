'use client';

import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useEffect } from 'react';
import { useListMyIssues } from '../../api/generated/cards/cards';
import { body } from '../../lib/api';
import { issueTitle, issueType } from '../../lib/issue';
import { IssueTypeIcon, PriorityIcon } from '../jira/issue-ui';
import { useWorkspaceCtx } from './workspace-context';

/** "Your work": issues assigned to you, and the projects of the workspace. */
export function WorkspaceHome({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('workspace');
  const ctx = useWorkspaceCtx();
  const mine = body(useListMyIssues(workspaceId).data) ?? [];

  useEffect(() => {
    try {
      localStorage.setItem('work.lastWorkspace', workspaceId);
    } catch {
      /* ignore */
    }
  }, [workspaceId]);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-6">
      <h1 className="text-2xl font-medium text-[#172B4D]" data-testid="workspace-title">{ctx.workspace?.name}</h1>

      <section aria-label={t('assignedToMe')}>
        <h2 className="mb-2 text-base font-semibold text-[#172B4D]">{t('assignedToMe')}</h2>
        {mine.length === 0 ? (
          <p className="text-muted" data-testid="no-assigned">{t('noAssigned')}</p>
        ) : (
          <ul className="divide-y divide-[#DCDFE4] rounded-[3px] border border-[#DCDFE4]" data-testid="assigned-issues">
            {mine.map((c) => {
              const type = issueType(c, ctx.cardTypes);
              return (
                <li key={c.id} className="flex items-center gap-3 px-3 py-2 text-sm hover:bg-[#F7F8F9]">
                  <IssueTypeIcon icon={type?.icon} color={type?.color} title={type?.name} />
                  <span className="w-20 shrink-0 text-xs font-medium text-[#626F86]">{c.key}</span>
                  <Link className="min-w-0 flex-1 truncate hover:text-[#0C66E4] hover:underline" href={`/workspaces/${workspaceId}/cards/${c.id}`}>
                    {issueTitle(c, ctx.cardTypes)}
                  </Link>
                  <PriorityIcon value={c.data.priority} />
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-label={t('lists')}>
        <h2 className="mb-2 text-base font-semibold text-[#172B4D]">{t('projects')}</h2>
        {ctx.lists.length === 0 && <p className="text-muted">{t('noLists')}</p>}
        <div className="grid gap-3 sm:grid-cols-2" data-testid="project-cards">
          {ctx.lists.map((l) => (
            <Link key={l.id} href={`/workspaces/${workspaceId}/lists/${l.slug}`} className="flex items-center gap-3 rounded-[3px] border border-[#DCDFE4] p-3 hover:bg-[#F7F8F9]">
              <span className="flex size-10 shrink-0 items-center justify-center rounded-[4px] bg-gradient-to-br from-[#1D7AFC] to-[#0C66E4] font-bold text-white">{l.name.slice(0, 2).toUpperCase()}</span>
              <span className="min-w-0">
                <span className="block truncate text-sm font-semibold">{l.name}</span>
                <span className="block truncate text-xs text-muted">{l.spaceName} · {l.methodology === 'scrum' ? t('scrum') : t('kanban')}</span>
              </span>
            </Link>
          ))}
        </div>
      </section>
    </div>
  );
}
