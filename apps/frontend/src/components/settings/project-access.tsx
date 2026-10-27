'use client';

import { Button, Chip, Label, Radio, RadioGroup, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Globe, Lock, Trash2, UserPlus } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  useCreateListInvite,
  useDeleteListGrant,
  useInviteToListByEmail,
  useListListGrants,
  usePutListGrant,
  useUpdateList,
} from '../../api/generated/lists/lists';
import type { ListDetail } from '../../api/generated/model';
import { useGetSystemInfo } from '../../api/generated/system/system';
import { body, errorKind, serverMessage } from '../../lib/api';
import { fullName } from '../../lib/format';
import { useMembers, useWorkspace } from '../../lib/hooks';
import { UserAvatar } from '../shell/user-avatar';
import { SimpleSelect } from '../ui/inputs';
import { EditScope, Panel, Stagger, nth } from '../ui/settings';
import { InviteDialog } from './invite-dialog';

const ROLES = ['viewer', 'member', 'admin'];

/**
 * Who can open this project. A public project is open to everyone in the workspace (except people
 * with limited access); a private one only to the people added here and to workspace admins.
 * People who are not in the workspace yet are invited from here and see only this project.
 */
export function ProjectAccess({
  list,
  workspaceId,
  canEdit,
}: {
  list: ListDetail;
  workspaceId: string;
  canEdit: boolean;
}) {
  const t = useTranslations('psettings');
  const tr = useTranslations('project');
  const qc = useQueryClient();
  const ws = useWorkspace(workspaceId);
  const members = useMembers(ws?.project_id);
  const grants = body(useListListGrants(list.id).data) ?? [];
  const update = useUpdateList();
  const put = usePutListGrant();
  const del = useDeleteListGrant();
  const inviteLink = useCreateListInvite();
  const inviteMail = useInviteToListByEmail();
  const mailEnabled = body(useGetSystemInfo().data)?.mail_enabled ?? false;
  const [pick, setPick] = useState<string | null>(null);
  const [role, setRole] = useState<string | null>('member');
  const [inviting, setInviting] = useState(false);
  const refresh = () => qc.invalidateQueries();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? errorKind(e));

  const isPrivate = list.access_type === 'private';
  const byId = new Map(members.map((m) => [m.user_id, m]));
  const admins = members.filter(
    (m) => m.role === 'admin' || m.role === 'owner',
  );
  const granted = grants
    .map((g) => ({ g, m: byId.get(g.user_id) }))
    .filter((x) => !!x.m && !admins.some((a) => a.user_id === x.g.user_id));
  const candidates = members.filter(
    (m) =>
      !admins.some((a) => a.user_id === m.user_id) &&
      !grants.some((g) => g.user_id === m.user_id),
  );
  const total = admins.length + granted.length;

  const setAccess = async (v: string) => {
    try {
      await update.mutateAsync({ id: list.id, data: { access_type: v } });
      await refresh();
      toast.success(t('saved'));
    } catch (e) {
      fail(e);
    }
  };

  return (
    <EditScope canEdit={canEdit}>
      <div className="flex flex-col gap-6">
        <Panel>
          <Panel.Header>
            <div>
              <Panel.Title>{t('visibility')}</Panel.Title>
              <Panel.Description>{t('visibilityHelp')}</Panel.Description>
            </div>
          </Panel.Header>
          <Panel.Content>
            <RadioGroup
              aria-label={t('visibility')}
              value={list.access_type}
              onChange={(v) => void setAccess(v)}
              className="gap-3"
              data-testid="visibility"
            >
              {(['public', 'private'] as const).map((v) => {
                const Icon = v === 'public' ? Globe : Lock;
                return (
                  <Radio
                    key={v}
                    value={v}
                    className="flex-row items-start gap-3 rounded-xl border border-separator p-3 transition-colors data-[selected=true]:border-accent data-[selected=true]:bg-accent-soft"
                    data-testid={`visibility-${v}`}
                  >
                    <Radio.Control>
                      <Radio.Indicator />
                    </Radio.Control>
                    <Radio.Content className="flex flex-col">
                      <Label className="flex items-center gap-2">
                        <Icon size={14} />
                        {t(`visibilityOptions.${v}.title`)}
                      </Label>
                      <p className="text-xs text-muted">
                        {t(`visibilityOptions.${v}.help`)}
                      </p>
                    </Radio.Content>
                  </Radio>
                );
              })}
            </RadioGroup>
          </Panel.Content>
        </Panel>

        <Panel testId="project-people">
          <Panel.Header>
            <div>
              <Panel.Title>{t('people')}</Panel.Title>
              <Panel.Description>
                {t('peopleCount', { count: total })}
              </Panel.Description>
            </div>
            {canEdit && (
              <Button
                size="sm"
                onPress={() => setInviting(true)}
                data-testid="invite-to-project"
              >
                <UserPlus size={14} />
                {t('invite')}
              </Button>
            )}
          </Panel.Header>
          <Stagger
            as="ul"
            className="divide-y divide-separator"
            testId="project-access-list"
          >
            {!isPrivate && (
              <li
                style={nth(0)}
                className="flex items-center gap-3 px-5 py-3 text-sm"
              >
                <span className="flex size-8 items-center justify-center rounded-full bg-default text-muted">
                  <Globe size={14} />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="font-medium">{t('everyone')}</p>
                  <p className="text-xs text-muted">{t('everyoneHelp')}</p>
                </div>
              </li>
            )}
            {admins.map((m, i) => (
              <li
                key={m.user_id}
                style={nth(i + 1)}
                className="flex items-center gap-3 px-5 py-3"
              >
                <UserAvatar user={m} />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{fullName(m)}</p>
                  <p className="truncate text-xs text-muted">{m.email}</p>
                </div>
                <Chip size="sm">
                  <Chip.Label>{t('workspaceAdmin')}</Chip.Label>
                </Chip>
              </li>
            ))}
            {granted.map(({ g, m }, i) => (
              <li
                key={g.user_id}
                style={nth(i + admins.length + 1)}
                className="flex items-center gap-3 px-5 py-3 transition-colors hover:bg-surface-secondary"
              >
                <UserAvatar user={m!} />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {fullName(m!)}
                    {m!.restricted && (
                      <Chip size="sm" className="ml-2">
                        <Chip.Label>{t('onlyThisProject')}</Chip.Label>
                      </Chip>
                    )}
                  </p>
                  <p className="truncate text-xs text-muted">{m!.email}</p>
                </div>
                <div className="w-36">
                  <SimpleSelect
                    hideLabel
                    label={tr('role')}
                    value={g.role}
                    options={ROLES.map((r) => ({
                      value: r,
                      label: tr(`roles.${r}`),
                    }))}
                    onChange={async (v) => {
                      if (!v) return;
                      try {
                        await put.mutateAsync({
                          id: list.id,
                          userId: g.user_id,
                          data: { role: v },
                        });
                        await refresh();
                      } catch (e) {
                        fail(e);
                      }
                    }}
                  />
                </div>
                <Button
                  isIconOnly
                  size="sm"
                  variant="ghost"
                  aria-label={t('removeAccess')}
                  onPress={async () => {
                    try {
                      await del.mutateAsync({ id: list.id, userId: g.user_id });
                      await refresh();
                    } catch (e) {
                      fail(e);
                    }
                  }}
                >
                  <Trash2 size={14} />
                </Button>
              </li>
            ))}
          </Stagger>
          {canEdit && candidates.length > 0 && (
            <div className="flex flex-wrap items-end gap-2 border-t border-separator px-5 py-4">
              <div className="min-w-48 flex-1">
                <SimpleSelect
                  label={t('addPerson')}
                  value={pick}
                  onChange={setPick}
                  options={candidates.map((m) => ({
                    value: m.user_id,
                    label: fullName(m),
                  }))}
                  testId="grant-person"
                />
              </div>
              <div className="w-36">
                <SimpleSelect
                  label={tr('role')}
                  value={role}
                  onChange={setRole}
                  options={ROLES.map((r) => ({
                    value: r,
                    label: tr(`roles.${r}`),
                  }))}
                />
              </div>
              <Button
                isDisabled={!pick}
                data-testid="grant-list"
                onPress={async () => {
                  if (!pick) return;
                  try {
                    await put.mutateAsync({
                      id: list.id,
                      userId: pick,
                      data: { role: role ?? 'member' },
                    });
                    setPick(null);
                    await refresh();
                    toast.success(t('added'));
                  } catch (e) {
                    fail(e);
                  }
                }}
              >
                {t('add')}
              </Button>
            </div>
          )}
        </Panel>

        <InviteDialog
          isOpen={inviting}
          onClose={() => setInviting(false)}
          roles={ROLES}
          mailEnabled={mailEnabled}
          onLink={async ({ role, hours, uses }) => {
            const res = await inviteLink.mutateAsync({
              id: list.id,
              data: { role, expires_in_hours: hours, max_uses: uses },
            });
            if (res.status !== 200) throw new Error('invite');
            return res.data;
          }}
          onEmail={async ({ email, role }) => {
            await inviteMail.mutateAsync({
              id: list.id,
              data: { email, role },
            });
          }}
        />
      </div>
    </EditScope>
  );
}
