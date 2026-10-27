'use client';

import { routes } from '../../lib/routes';
import { Button, Chip, Label, Switch, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { ExternalLink, Lock, Plus, Settings, Trash2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useState } from 'react';
import {
  useDeleteSpace,
  useDeleteSpaceGrant,
  useGetWorkspaceTree,
  useListSpaceGrants,
  usePutSpaceGrant,
  useUpdateSpace,
} from '../../api/generated/spaces/spaces';
import type { SpaceTree } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName } from '../../lib/format';
import { useMembers, useWorkspace } from '../../lib/hooks';
import { CreateSpaceDialog } from '../shell/create-space-dialog';
import { SimpleSelect } from '../ui/inputs';
import { Panel, Stagger, nth } from '../ui/settings';

/**
 * The spaces of the workspace and the projects in them. A space groups projects and can be
 * private; what a single project allows and how it works is set on the project itself.
 */
export function SpaceListSettings({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('settings');
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  const [creating, setCreating] = useState(false);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between gap-4">
        <p className="text-sm text-muted">{t('spacesExplain')}</p>
        <Button
          size="sm"
          onPress={() => setCreating(true)}
          data-testid="new-space"
        >
          <Plus size={14} />
          {t('newSpace')}
        </Button>
      </div>
      <CreateSpaceDialog
        workspaceId={workspaceId}
        isOpen={creating}
        onClose={() => setCreating(false)}
      />
      {tree?.spaces.length === 0 && (
        <p className="text-muted">{t('noSpaces')}</p>
      )}
      <Stagger className="flex flex-col gap-6">
        {tree?.spaces.map((s, i) => (
          <div key={s.id} style={nth(i)}>
            <SpaceCard workspaceId={workspaceId} space={s} />
          </div>
        ))}
      </Stagger>
    </div>
  );
}

type Space = SpaceTree;

function GrantEditor({ id, projectId }: { id: string; projectId?: string }) {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const members = useMembers(projectId);
  const grants = body(useListSpaceGrants(id).data) ?? [];
  const putSpace = usePutSpaceGrant();
  const delSpace = useDeleteSpaceGrant();
  const [pick, setPick] = useState<string | null>(null);
  const [role, setRole] = useState<string | null>('member');
  const candidates = members.filter(
    (m) => !grants.some((g) => g.user_id === m.user_id),
  );
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  return (
    <div
      className="flex flex-col gap-2 rounded-xl bg-surface-secondary p-4"
      data-testid="grants-space"
    >
      <p className="text-sm font-medium">{t('access')}</p>
      <ul className="text-sm">
        {grants.map((g) => {
          const m = members.find((x) => x.user_id === g.user_id);
          return (
            <li
              key={g.user_id}
              className="flex items-center justify-between py-1"
            >
              <span>
                {m ? fullName(m) : g.user_id} · {t(`roles.${g.role}`)}
              </span>
              <Button
                isIconOnly
                size="sm"
                variant="ghost"
                aria-label={t('revoke')}
                onPress={async () => {
                  try {
                    await delSpace.mutateAsync({ id, userId: g.user_id });
                    await refresh();
                  } catch (e) {
                    fail(e);
                  }
                }}
              >
                <Trash2 size={12} />
              </Button>
            </li>
          );
        })}
      </ul>
      {candidates.length > 0 && (
        <div className="flex items-end gap-2">
          <div className="flex-1">
            <SimpleSelect
              label={t('person')}
              value={pick}
              onChange={setPick}
              options={candidates.map((m) => ({
                value: m.user_id,
                label: fullName(m),
              }))}
            />
          </div>
          <div className="w-32">
            <SimpleSelect
              label={t('role')}
              value={role}
              onChange={setRole}
              options={['viewer', 'member', 'admin'].map((r) => ({
                value: r,
                label: t(`roles.${r}`),
              }))}
            />
          </div>
          <Button
            size="sm"
            isDisabled={!pick}
            onPress={async () => {
              if (!pick) return;
              try {
                await putSpace.mutateAsync({
                  id,
                  userId: pick,
                  data: { role: role ?? 'member' },
                });
                setPick(null);
                await refresh();
              } catch (e) {
                fail(e);
              }
            }}
            data-testid="grant-space"
          >
            {t('grant')}
          </Button>
        </div>
      )}
    </div>
  );
}

function SpaceCard({
  workspaceId,
  space,
}: {
  workspaceId: string;
  space: Space;
}) {
  const t = useTranslations('settings');
  const tp = useTranslations('wsettings');
  const qc = useQueryClient();
  const workspace = useWorkspace(workspaceId);
  const update = useUpdateSpace();
  const del = useDeleteSpace();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));
  return (
    <Panel testId={`space-${space.name}`}>
      <Panel.Header className="items-center">
        <div className="flex items-center gap-2">
          <Panel.Title>{space.name}</Panel.Title>
          {space.access_type === 'private' && (
            <Lock size={12} className="text-muted" aria-label={t('private')} />
          )}
        </div>
        <div className="flex items-center gap-3">
          <Switch
            className="flex flex-row items-center gap-2"
            isSelected={space.access_type === 'private'}
            onChange={async (v) => {
              try {
                await update.mutateAsync({
                  id: space.id,
                  data: { access_type: v ? 'private' : 'public' },
                });
                await qc.invalidateQueries();
              } catch (e) {
                fail(e);
              }
            }}
            data-testid={`private-${space.name}`}
          >
            <Switch.Control>
              <Switch.Thumb />
            </Switch.Control>
            <Switch.Content>
              <Label>{t('private')}</Label>
            </Switch.Content>
          </Switch>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={t('deleteSpace')}
            onPress={async () => {
              if (!window.confirm(t('confirmDeleteSpace'))) return;
              try {
                await del.mutateAsync({ id: space.id });
                await qc.invalidateQueries();
              } catch (e) {
                fail(e);
              }
            }}
          >
            <Trash2 size={14} />
          </Button>
        </div>
      </Panel.Header>
      {space.access_type === 'private' && (
        <Panel.Content className="border-b border-separator">
          <GrantEditor id={space.id} projectId={workspace?.project_id} />
        </Panel.Content>
      )}
      {space.lists.length === 0 ? (
        <Panel.Content>
          <p className="text-sm text-muted">{tp('noProjects')}</p>
        </Panel.Content>
      ) : (
        <ul className="divide-y divide-separator">
          {space.lists.map((l) => (
            <li
              key={l.id}
              data-testid={`list-settings-${l.name}`}
              className="flex items-center gap-3 px-5 py-3 transition-colors hover:bg-surface-secondary"
            >
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-xs font-bold text-accent">
                {l.name.slice(0, 2).toUpperCase()}
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{l.name}</p>
                <p className="text-xs text-muted">
                  {l.methodology === 'scrum' ? tp('scrum') : tp('kanban')}
                </p>
              </div>
              <Chip size="sm">
                <Chip.Label>
                  {l.access_type === 'private' ? t('private') : t('public')}
                </Chip.Label>
              </Chip>
              <Link
                href={routes.project(workspace?.key_prefix ?? '', l.slug)}
                aria-label={tp('openProject')}
                className="text-muted transition-colors hover:text-foreground"
              >
                <ExternalLink size={14} />
              </Link>
              <Link
                href={routes.projectSettings(
                  workspace?.key_prefix ?? '',
                  l.slug,
                )}
                className="inline-flex h-8 items-center gap-1.5 rounded-md px-2 text-sm text-accent transition-colors hover:bg-accent-soft"
              >
                <Settings size={14} />
                {tp('projectSettings')}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
