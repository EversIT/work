'use client';

import { routes } from '../../lib/routes';
import { Info, ListChecks, Shield, SlidersHorizontal, Zap } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { notFound } from 'next/navigation';
import { useGetListBySlug } from '../../api/generated/lists/lists';
import { body } from '../../lib/api';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { AutomationList } from '../automations/automation-list';
import { ProjectAccess } from './project-access';
import { ProjectDetails } from './project-details';
import { ProjectFields } from './project-fields';
import { ProjectWorkflow } from './project-workflow';
import { SettingsShell, type NavGroup } from './settings-shell';

const SECTIONS = [
  'details',
  'access',
  'workflow',
  'fields',
  'automations',
] as const;
export type ProjectSection = (typeof SECTIONS)[number];

function projectSectionFrom(segment?: string): ProjectSection {
  if (!segment) return 'details';
  const hit = SECTIONS.find((s) => s === segment);
  if (!hit) notFound();
  return hit;
}

const ICON = {
  details: Info,
  access: Shield,
  workflow: ListChecks,
  fields: SlidersHorizontal,
  automations: Zap,
} as const;

const TEST_ID = {
  details: 'ptab-details',
  access: 'ptab-access',
  workflow: 'ptab-workflow',
  fields: 'ptab-fields',
  automations: 'ptab-automations',
} as const;

/**
 * The settings of one project (what the sidebar calls a project: a board or a backlog). Name,
 * who may open it, its statuses and its fields. What is shared by every project of the workspace
 * (issue types, spaces, people in the workspace) stays in the workspace settings.
 */
export function ProjectSettingsShell({
  workspaceId,
  slug,
  segment,
}: {
  workspaceId: string;
  slug: string;
  segment?: string;
}) {
  const section = projectSectionFrom(segment);
  const t = useTranslations('psettings');
  const ctx = useWorkspaceCtx();
  const brief = ctx.lists.find((l) => l.slug === slug);
  const list = body(useGetListBySlug(workspaceId, slug).data);
  const wsKey = ctx.workspace?.key_prefix ?? '';
  const base = routes.project(wsKey, slug);
  const href = (s: ProjectSection) =>
    routes.projectSettings(wsKey, slug, s === 'details' ? undefined : s);
  const item = (s: ProjectSection) => ({
    id: s,
    href: href(s),
    icon: ICON[s],
    label: t(`nav.${s}`),
    testId: TEST_ID[s],
  });
  const groups: NavGroup[] = [
    { label: t('group.project'), items: [item('details'), item('access')] },
    {
      label: t('group.work'),
      items: [item('workflow'), item('fields'), item('automations')],
    },
  ];
  const name = list?.name ?? brief?.name ?? '…';
  const isAdmin = list?.role === 'admin' || list?.role === 'owner';

  return (
    <SettingsShell
      back={{ href: base, label: t('back'), testId: 'back-to-project' }}
      identity={{
        initials: name.slice(0, 2).toUpperCase(),
        name,
        caption: t('title'),
      }}
      groups={groups}
      activeId={section}
      trail={`${name} / ${t('title')}`}
      page={{
        icon: ICON[section],
        title: t(`page.${section}.title`),
        description: t(`page.${section}.description`),
      }}
      ready={!!list}
    >
      {list && !isAdmin && (
        <p
          className="mb-4 rounded-lg bg-warning/10 px-4 py-3 text-sm text-foreground"
          data-testid="settings-readonly"
        >
          {t('adminOnly')}
        </p>
      )}
      {list && section === 'details' && (
        <ProjectDetails list={list} canEdit={isAdmin} />
      )}
      {list && section === 'access' && (
        <ProjectAccess
          list={list}
          workspaceId={workspaceId}
          canEdit={isAdmin}
        />
      )}
      {list && section === 'workflow' && (
        <ProjectWorkflow list={list} canEdit={isAdmin} />
      )}
      {list && section === 'fields' && (
        <ProjectFields
          list={list}
          workspaceId={workspaceId}
          canEdit={isAdmin}
        />
      )}
      {list && section === 'automations' && (
        <AutomationList workspaceId={workspaceId} listId={list.id} />
      )}
    </SettingsShell>
  );
}
