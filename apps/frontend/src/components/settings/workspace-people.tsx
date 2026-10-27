'use client';

import { Button, Chip, Label, Switch, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import {
  Eye,
  Pencil,
  ShieldCheck,
  Crown,
  Trash2,
  UserPlus,
} from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useInviteByEmail } from '../../api/generated/projects/projects';
import {
  useCreateProjectInvite,
  useDeleteProject,
  useGetProject,
  useListProjectMembers,
  useRemoveProjectMember,
  useSetProjectMemberRole,
} from '../../api/generated/projects/projects';
import { useGetSystemInfo } from '../../api/generated/system/system';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName } from '../../lib/format';
import { useCurrentUser } from '../../lib/hooks';
import { UserAvatar } from '../shell/user-avatar';
import { SimpleSelect } from '../ui/inputs';
import { InviteDialog } from './invite-dialog';
import {
  ConfirmByTyping,
  Panel,
  SettingsRow,
  Stagger,
  nth,
} from '../ui/settings';

const RANK: Record<string, number> = {
  viewer: 0,
  member: 1,
  admin: 2,
  owner: 3,
};

const ROLE_ICON = {
  viewer: Eye,
  member: Pencil,
  admin: ShieldCheck,
  owner: Crown,
} as const;

/** Who is in the workspace and what they may do, with invitations and the way out. */
export function WorkspacePeople({ projectId }: { projectId: string }) {
  const t = useTranslations('project');
  const tp = useTranslations('wsettings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const me = useCurrentUser();
  const project = body(useGetProject(projectId).data);
  const members = body(useListProjectMembers(projectId).data) ?? [];
  const setRole = useSetProjectMemberRole();
  const remove = useRemoveProjectMember();
  const invite = useCreateProjectInvite();
  const inviteMail = useInviteByEmail();
  const mailEnabled = body(useGetSystemInfo().data)?.mail_enabled ?? false;
  const delProject = useDeleteProject();
  const [inviteOpen, setInviteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));

  if (!project) return null;
  const myRank = RANK[project.role] ?? 0;
  const canManage = myRank >= RANK.admin;
  const roles = ['viewer', 'member', 'admin'].filter((r) => RANK[r] < myRank);

  return (
    <div className="flex flex-col gap-6" data-testid="project-title">
      <Panel>
        <Panel.Header>
          <div>
            <Panel.Title>{t('members')}</Panel.Title>
            <Panel.Description>
              {tp('people', { count: members.length })}
            </Panel.Description>
          </div>
          {canManage && (
            <Button
              size="sm"
              onPress={() => {
                setInviteOpen(true);
              }}
              data-testid="invite"
            >
              <UserPlus size={14} />
              {t('invite')}
            </Button>
          )}
        </Panel.Header>
        <Stagger as="ul" className="divide-y divide-border" testId="members">
          {members.map((m, i) => {
            const mine = m.user_id === me?.id;
            const editable = canManage && !mine && RANK[m.role] < myRank;
            const RoleIcon = ROLE_ICON[m.role as keyof typeof ROLE_ICON] ?? Eye;
            return (
              <li
                key={m.user_id}
                style={nth(i)}
                className="flex items-center gap-3 px-5 py-3 transition-colors hover:bg-surface-secondary"
              >
                <UserAvatar user={m} />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {fullName(m)}
                    {mine && (
                      <span className="ml-1.5 text-xs font-normal text-muted">
                        ({t('you')})
                      </span>
                    )}
                  </p>
                  <p className="truncate text-xs text-muted">{m.email}</p>
                </div>
                {editable ? (
                  <div className="w-40">
                    <SimpleSelect
                      hideLabel
                      label={t('role')}
                      value={m.role}
                      options={roles.map((r) => ({
                        value: r,
                        label: t(`roles.${r}`),
                      }))}
                      onChange={async (v) => {
                        if (!v) return;
                        try {
                          await setRole.mutateAsync({
                            id: projectId,
                            userId: m.user_id,
                            data: { role: v },
                          });
                          await qc.invalidateQueries();
                          toast.success(tp('roleChanged'));
                        } catch (e) {
                          fail(e);
                        }
                      }}
                    />
                  </div>
                ) : (
                  <Chip
                    size="sm"
                    color={m.role === 'owner' ? 'accent' : 'default'}
                  >
                    <RoleIcon size={12} />
                    <Chip.Label>{t(`roles.${m.role}`)}</Chip.Label>
                  </Chip>
                )}
                {editable && m.role === 'viewer' && (
                  <Switch
                    className="flex flex-row items-center gap-2"
                    isSelected={m.restricted}
                    onChange={async (v) => {
                      try {
                        await setRole.mutateAsync({
                          id: projectId,
                          userId: m.user_id,
                          data: { restricted: v },
                        });
                        await qc.invalidateQueries();
                      } catch (e) {
                        fail(e);
                      }
                    }}
                    data-testid={`limited-${m.email}`}
                  >
                    <Switch.Control>
                      <Switch.Thumb />
                    </Switch.Control>
                    <Switch.Content>
                      <Label className="text-xs">{tp('limited')}</Label>
                    </Switch.Content>
                  </Switch>
                )}
                {(editable || (mine && m.role !== 'owner')) && (
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={t('removeMember')}
                    onPress={async () => {
                      if (
                        !window.confirm(
                          mine ? t('confirmLeave') : t('confirmRemove'),
                        )
                      )
                        return;
                      try {
                        await remove.mutateAsync({
                          id: projectId,
                          userId: m.user_id,
                        });
                        await qc.invalidateQueries();
                        if (mine) router.replace('/');
                      } catch (e) {
                        fail(e);
                      }
                    }}
                  >
                    <Trash2 size={14} />
                  </Button>
                )}
              </li>
            );
          })}
        </Stagger>
      </Panel>

      <Panel>
        <Panel.Header>
          <div>
            <Panel.Title>{tp('rolesTitle')}</Panel.Title>
            <Panel.Description>{tp('rolesHelp')}</Panel.Description>
          </div>
        </Panel.Header>
        {(['viewer', 'member', 'admin', 'owner'] as const).map((r) => {
          const Icon = ROLE_ICON[r];
          return (
            <SettingsRow
              key={r}
              label={t(`roles.${r}`)}
              help={tp(`roleHelp.${r}`)}
            >
              <span className="inline-flex size-7 items-center justify-center rounded-md bg-default text-muted">
                <Icon size={14} />
              </span>
            </SettingsRow>
          );
        })}
      </Panel>

      {project.role === 'owner' && (
        <Panel tone="danger">
          <Panel.Header className="border-danger/30">
            <Panel.Title className="text-danger">{t('dangerZone')}</Panel.Title>
          </Panel.Header>
          <SettingsRow
            label={t('deleteProject')}
            help={tp('deleteProjectHelp')}
          >
            <Button
              variant="danger-soft"
              data-testid="delete-project"
              onPress={() => setDeleting(true)}
            >
              {t('deleteProject')}
            </Button>
          </SettingsRow>
        </Panel>
      )}

      <ConfirmByTyping
        isOpen={deleting}
        onClose={() => setDeleting(false)}
        title={tp('confirmTitle', { name: project.name })}
        help={t('confirmDeleteProject')}
        expected={project.name}
        confirmLabel={t('deleteProject')}
        onConfirm={async () => {
          await delProject.mutateAsync({ id: projectId });
          await qc.invalidateQueries();
          router.replace('/');
        }}
      />

      <InviteDialog
        isOpen={inviteOpen}
        onClose={() => setInviteOpen(false)}
        roles={roles}
        mailEnabled={mailEnabled}
        onLink={async ({ role, hours, uses }) => {
          const res = await invite.mutateAsync({
            id: projectId,
            data: { role, expires_in_hours: hours, max_uses: uses },
          });
          if (res.status !== 200) throw new Error('invite');
          return res.data;
        }}
        onEmail={async ({ email, role }) => {
          await inviteMail.mutateAsync({
            id: projectId,
            data: { email, role },
          });
        }}
      />
    </div>
  );
}
