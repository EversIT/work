'use client';

import { Button, Card, Chip, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Copy, Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useGetSystemInfo } from '../../api/generated/system/system';
import { useInviteByEmail } from '../../api/generated/projects/projects';
import { useCreateProjectInvite, useDeleteProject, useGetProject, useListProjectMembers, useRemoveProjectMember, useSetProjectMemberRole } from '../../api/generated/projects/projects';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate, fullName } from '../../lib/format';
import { useCurrentUser } from '../../lib/hooks';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { UserAvatar } from '../shell/user-avatar';

const RANK: Record<string, number> = { viewer: 0, member: 1, admin: 2, owner: 3 };

export function ProjectSettings({ projectId }: { projectId: string }) {
  const t = useTranslations('project');
  const te = useTranslations('errors');
  const locale = useLocale();
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
  const [mailInvited, setMailInvited] = useState(false);
  const delProject = useDeleteProject();
  const [inviteOpen, setInviteOpen] = useState(false);
  const [role, setRole_] = useState<string | null>('member');
  const [link, setLink] = useState<{ url: string; expires: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));

  if (!project) return null;
  const myRank = RANK[project.role] ?? 0;
  const canManage = myRank >= RANK.admin;
  const roles = ['viewer', 'member', 'admin'].filter((r) => RANK[r] < myRank);

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6 p-4">
      <h1 className="text-xl font-semibold" data-testid="project-title">{project.name}</h1>
      <Card>
        <Card.Header className="flex-row items-center justify-between">
          <Card.Title>{t('members')}</Card.Title>
          {canManage && <Button size="sm" onPress={() => { setLink(null); setMailInvited(false); setInviteOpen(true); }} data-testid="invite">{t('invite')}</Button>}
        </Card.Header>
        <Card.Content>
          <ul className="divide-y divide-separator" data-testid="members">
            {members.map((m) => {
              const mine = m.user_id === me?.id;
              const editable = canManage && !mine && RANK[m.role] < myRank;
              return (
                <li key={m.user_id} className="flex items-center gap-3 py-2">
                  <UserAvatar user={m} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{fullName(m)}{mine ? ` (${t('you')})` : ''}</p>
                    <p className="truncate text-xs text-muted">{m.email}</p>
                  </div>
                  {editable ? (
                    <div className="w-36">
                      <SimpleSelect label={t('role')} value={m.role} options={roles.map((r) => ({ value: r, label: t(`roles.${r}`) }))}
                        onChange={async (v) => {
                          if (!v) return;
                          try {
                            await setRole.mutateAsync({ id: projectId, userId: m.user_id, data: { role: v } });
                            await qc.invalidateQueries();
                          } catch (e) { fail(e); }
                        }} />
                    </div>
                  ) : (
                    <Chip size="sm"><Chip.Label>{t(`roles.${m.role}`)}</Chip.Label></Chip>
                  )}
                  {(editable || (mine && m.role !== 'owner')) && (
                    <Button isIconOnly size="sm" variant="ghost" aria-label={t('removeMember')}
                      onPress={async () => {
                        if (!window.confirm(mine ? t('confirmLeave') : t('confirmRemove'))) return;
                        try {
                          await remove.mutateAsync({ id: projectId, userId: m.user_id });
                          await qc.invalidateQueries();
                          if (mine) router.replace('/');
                        } catch (e) { fail(e); }
                      }}>
                      <Trash2 size={14} />
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        </Card.Content>
      </Card>

      {project.role === 'owner' && (
        <Card>
          <Card.Header><Card.Title>{t('dangerZone')}</Card.Title></Card.Header>
          <Card.Content>
            <Button variant="danger-soft" data-testid="delete-project"
              onPress={async () => {
                if (!window.confirm(t('confirmDeleteProject'))) return;
                try {
                  await delProject.mutateAsync({ id: projectId });
                  await qc.invalidateQueries();
                  router.replace('/');
                } catch (e) { fail(e); }
              }}>
              {t('deleteProject')}
            </Button>
          </Card.Content>
        </Card>
      )}

      <FormDialog
        isOpen={inviteOpen}
        onClose={() => setInviteOpen(false)}
        title={t('invite')}
        submitLabel={link || mailInvited ? t('close') : t('createInvite')}
        error={error}
        isPending={invite.isPending}
        onSubmit={async (f) => {
          if (link || mailInvited) return setInviteOpen(false);
          setError(null);
          try {
            const email = String(f.get('email') ?? '').trim();
            if (email) {
              await inviteMail.mutateAsync({ id: projectId, data: { email, role: role ?? 'member' } });
              setMailInvited(true);
              return;
            }
            const res = await invite.mutateAsync({ id: projectId, data: { role: role ?? 'member', expires_in_hours: Number(f.get('hours') ?? 72), max_uses: Number(f.get('uses') ?? 1) } });
            if (res.status === 200) setLink({ url: `${window.location.origin}/invite?token=${encodeURIComponent(res.data.token)}`, expires: res.data.expires_at });
          } catch (e) {
            setError(serverMessage(e) ?? te(errorKind(e)));
          }
        }}
      >
        {link ? (
          <div className="flex flex-col gap-2">
            <p className="text-sm">{t('inviteReady', { date: formatDate(link.expires, locale, true) })}</p>
            <div className="flex items-center gap-2">
              <input readOnly aria-label={t('inviteLink')} value={link.url} className="h-9 flex-1 rounded-lg border border-field-border bg-field-background px-2 text-xs" data-testid="invite-link" onFocus={(e) => e.currentTarget.select()} />
              <Button isIconOnly variant="tertiary" aria-label={t('copy')} onPress={() => navigator.clipboard?.writeText(link.url).then(() => toast.success(t('copied')))}>
                <Copy size={14} />
              </Button>
            </div>
            <p className="text-xs text-muted">{t('inviteOnce')}</p>
          </div>
        ) : mailInvited ? (
          <p className="text-sm" data-testid="mail-invited">{t('mailSent')}</p>
        ) : (
          <>
            <SimpleSelect label={t('role')} value={role} onChange={setRole_} options={roles.map((r) => ({ value: r, label: t(`roles.${r}`) }))} testId="invite-role" />
            <TextInput label={t('expiresInHours')} name="hours" type="number" defaultValue="72" required />
            <TextInput label={t('maxUses')} name="uses" type="number" defaultValue="1" required />
            {mailEnabled && (
              <div className="rounded-[3px] bg-[#F7F8F9] p-3">
                <TextInput label={t('inviteEmail')} name="email" type="email" placeholder={t('inviteEmailHint')} />
                <p className="mt-1 text-xs text-muted">{t('inviteEmailExplain')}</p>
              </div>
            )}
          </>
        )}
      </FormDialog>
    </div>
  );
}
