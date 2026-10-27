'use client';

import { Spinner } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useResolveIssue } from '../../api/generated/cards/cards';
import { body } from '../../lib/api';
import { AutomationBuilder } from '../automations/automation-builder';
import { CardPage } from '../cards/card-page';
import { ProjectSettingsShell } from '../settings/project-settings-shell';
import { WorkspaceSettingsShell } from '../settings/workspace-settings-shell';
import { BacklogPage } from '../views/backlog-page';
import { ListPage } from '../views/list-page';
import { useWorkspaceCtx } from './workspace-context';
import { WorkspaceHome } from './workspace-home';

/*
 * The pages are addressed by the workspace key (`/projects/ACME/...`); the shell has already
 * turned that into the workspace and offers it through the context. These thin components hand the
 * workspace to the page, so the pages themselves keep working with an id.
 */

export function WorkspaceHomeRoute() {
  const { workspaceId } = useWorkspaceCtx();
  return <WorkspaceHome workspaceId={workspaceId} />;
}

export function ListRoute({ slug }: { slug: string }) {
  const { workspaceId } = useWorkspaceCtx();
  return <ListPage workspaceId={workspaceId} slug={slug} />;
}

export function BacklogRoute({ slug }: { slug: string }) {
  const { workspaceId } = useWorkspaceCtx();
  return <BacklogPage workspaceId={workspaceId} slug={slug} />;
}

export function WorkspaceSettingsRoute({ segment }: { segment?: string }) {
  const { workspaceId } = useWorkspaceCtx();
  return <WorkspaceSettingsShell workspaceId={workspaceId} segment={segment} />;
}

export function ProjectSettingsRoute({
  slug,
  segment,
}: {
  slug: string;
  segment?: string;
}) {
  const { workspaceId } = useWorkspaceCtx();
  return (
    <ProjectSettingsShell
      workspaceId={workspaceId}
      slug={slug}
      segment={segment}
    />
  );
}

export function AutomationRoute({
  id,
  listId,
}: {
  id: string;
  listId?: string;
}) {
  const { workspaceId } = useWorkspaceCtx();
  return (
    <AutomationBuilder workspaceId={workspaceId} id={id} listId={listId} />
  );
}

/** An issue, by its key (`ACME-12`), from `/browse/ACME-12` or `/projects/ACME/issue/ACME-12`. */
export function IssueRoute({ issueKey }: { issueKey: string }) {
  const t = useTranslations('issue');
  const { workspaceId } = useWorkspaceCtx();
  const found = useResolveIssue(issueKey, { query: { retry: false } });
  const card = body(found.data);
  if (found.isError)
    return (
      <p className="p-8 text-center text-muted" data-testid="issue-not-found">
        {t('notFound')}
      </p>
    );
  if (!card)
    return (
      <div className="flex justify-center p-12">
        <Spinner aria-label={t('loading')} />
      </div>
    );
  return <CardPage workspaceId={workspaceId} cardId={card.id} />;
}
