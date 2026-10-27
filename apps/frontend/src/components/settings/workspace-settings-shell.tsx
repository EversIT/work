'use client';

import { routes } from '../../lib/routes';
import { FolderTree, Info, Layers, Users, Zap } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { notFound } from 'next/navigation';
import { useWorkspace } from '../../lib/hooks';
import { AutomationList } from '../automations/automation-list';
import { CardTypeSettings } from './card-type-settings';
import { WorkspaceDetails } from './workspace-details';
import { SettingsShell, type NavGroup } from './settings-shell';
import { SpaceListSettings } from './space-list-settings';
import { WorkspacePeople } from './workspace-people';

const SECTIONS = [
  'details',
  'people',
  'issue-types',
  'spaces',
  'automations',
] as const;
export type WorkspaceSection = (typeof SECTIONS)[number];

function workspaceSectionFrom(segment?: string): WorkspaceSection {
  if (!segment) return 'details';
  const hit = SECTIONS.find((s) => s === segment);
  if (!hit) notFound();
  return hit;
}

const ICON = {
  details: Info,
  people: Users,
  'issue-types': Layers,
  spaces: FolderTree,
  automations: Zap,
} as const;

const TEST_ID = {
  details: 'tab-general',
  people: 'tab-people',
  'issue-types': 'tab-types',
  spaces: 'tab-spaces',
  automations: 'tab-automations',
} as const;

/**
 * Settings that belong to the whole workspace: its name, the people in it, the issue types every
 * project shares, the spaces projects live in, and automations. A project's own settings (its
 * statuses, its access) are on the project, see `ProjectSettingsShell`.
 */
export function WorkspaceSettingsShell({
  workspaceId,
  segment,
}: {
  workspaceId: string;
  segment?: string;
}) {
  const section = workspaceSectionFrom(segment);
  const t = useTranslations('wsettings');
  const ws = useWorkspace(workspaceId);
  const wsKey = ws?.key_prefix ?? '';
  const href = (s: WorkspaceSection) =>
    routes.workspaceSettings(wsKey, s === 'details' ? undefined : s);
  const item = (s: WorkspaceSection) => ({
    id: s,
    href: href(s),
    icon: ICON[s],
    label: t(`nav.${s}`),
    testId: TEST_ID[s],
  });
  const groups: NavGroup[] = [
    { label: t('group.workspace'), items: [item('details'), item('people')] },
    { label: t('group.work'), items: [item('issue-types')] },
    { label: t('group.structure'), items: [item('spaces')] },
    { label: t('group.automation'), items: [item('automations')] },
  ];

  return (
    <SettingsShell
      back={{
        href: routes.workspace(wsKey),
        label: t('back'),
        testId: 'back-to-workspace',
      }}
      identity={{
        initials: (ws?.name ?? '··').slice(0, 2).toUpperCase(),
        name: ws?.name ?? '…',
        caption: t('title'),
      }}
      groups={groups}
      activeId={section}
      trail={`${ws?.name ?? ''} / ${t('title')}`}
      page={{
        icon: ICON[section],
        title: t(`page.${section}.title`),
        description: t(`page.${section}.description`),
      }}
      ready={!!ws}
    >
      {ws && section === 'details' && (
        <WorkspaceDetails workspaceId={workspaceId} />
      )}
      {ws && section === 'people' && (
        <WorkspacePeople projectId={ws.project_id} />
      )}
      {section === 'issue-types' && (
        <CardTypeSettings workspaceId={workspaceId} />
      )}
      {section === 'spaces' && <SpaceListSettings workspaceId={workspaceId} />}
      {section === 'automations' && (
        <AutomationList workspaceId={workspaceId} />
      )}
    </SettingsShell>
  );
}
