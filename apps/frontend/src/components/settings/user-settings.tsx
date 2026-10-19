'use client';

import { Alert, Button, Card, Switch, toast, Label } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useChangePassword, useDeleteMe, useMe, useUpdateMe } from '../../api/generated/auth/auth';
import { useGetSlackAuthUrl, useListIntegrations, useListSlackChannels, useSlackDisconnect } from '../../api/generated/integrations/integrations';
import { useGetNotificationPreferences, usePutNotificationPreference } from '../../api/generated/notifications/notifications';
import { setAccessToken } from '../../api/fetcher';
import { body, errorKind, serverMessage } from '../../lib/api';
import { LanguageSwitcher } from '../shell/language-switcher';
import { ApiTokenSettings } from './api-token-settings';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { FormDialog } from '../ui/form-dialog';

export function UserSettings() {
  const t = useTranslations('settings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const me = body(useMe().data);
  const updateMe = useUpdateMe();
  const changePw = useChangePassword();
  const deleteMe = useDeleteMe();
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const [pwError, setPwError] = useState<string | null>(null);
  const [del, setDel] = useState(false);
  const [delError, setDelError] = useState<string | null>(null);

  if (!me) return null;
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-6 p-4">
      <h1 className="text-xl font-semibold">{t('title')}</h1>

      <Card>
        <Card.Header><Card.Title>{t('profile')}</Card.Title></Card.Header>
        <Card.Content>
          <form
            className="flex flex-col gap-3"
            onSubmit={async (e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              try {
                await updateMe.mutateAsync({ data: { first_name: String(f.get('first')), last_name: String(f.get('last')) } });
                await qc.invalidateQueries();
                toast.success(t('saved'));
              } catch (err) {
                fail(err);
              }
            }}
          >
            <TextInput label={t('firstName')} name="first" defaultValue={me.first_name} required maxLength={100} />
            <TextInput label={t('lastName')} name="last" defaultValue={me.last_name} required maxLength={100} />
            <TextInput label={t('email')} value={me.email} type="email" />
            <Button type="submit" className="self-start" isPending={updateMe.isPending}>{t('save')}</Button>
          </form>
        </Card.Content>
      </Card>

      <Card>
        <Card.Header><Card.Title>{t('language')}</Card.Title></Card.Header>
        <Card.Content><LanguageSwitcher /></Card.Content>
      </Card>

      <NotificationSettings />
      <SlackSettings />
      <ApiTokenSettings />

      <Card>
        <Card.Header><Card.Title>{t('password')}</Card.Title></Card.Header>
        <Card.Content>
          <form
            className="flex flex-col gap-3"
            onSubmit={async (e) => {
              e.preventDefault();
              setPwError(null);
              const form = e.currentTarget;
              const f = new FormData(form);
              try {
                await changePw.mutateAsync({ data: { current_password: String(f.get('current')), new_password: String(f.get('next')) } });
                form.reset();
                toast.success(t('passwordChanged'));
              } catch (err) {
                setPwError(errorKind(err) === 'forbidden' ? t('wrongPassword') : serverMessage(err) ?? te(errorKind(err)));
              }
            }}
          >
            {pwError && <Alert status="danger"><Alert.Content><Alert.Description>{pwError}</Alert.Description></Alert.Content></Alert>}
            <TextInput label={t('currentPassword')} name="current" type="password" required />
            <TextInput label={t('newPassword')} name="next" type="password" required maxLength={128} />
            <p className="text-xs text-muted">{t('passwordHint')}</p>
            <Button type="submit" className="self-start" isPending={changePw.isPending} data-testid="change-password">{t('changePassword')}</Button>
          </form>
        </Card.Content>
      </Card>

      <Card>
        <Card.Header><Card.Title>{t('dangerZone')}</Card.Title></Card.Header>
        <Card.Content>
          <p className="mb-3 text-sm text-muted">{t('deleteExplain')}</p>
          <Button variant="danger-soft" onPress={() => setDel(true)} data-testid="delete-account">{t('deleteAccount')}</Button>
        </Card.Content>
      </Card>
      <FormDialog
        isOpen={del}
        onClose={() => setDel(false)}
        title={t('deleteAccount')}
        submitLabel={t('deleteAccount')}
        danger
        error={delError}
        isPending={deleteMe.isPending}
        onSubmit={async (f) => {
          setDelError(null);
          try {
            await deleteMe.mutateAsync({ data: { password: String(f.get('password')) } });
            setAccessToken(null);
            qc.clear();
            router.replace('/login');
          } catch (err) {
            const k = errorKind(err);
            setDelError(k === 'forbidden' ? t('wrongPassword') : k === 'conflict' ? t('deleteBlocked') : te(k));
          }
        }}
      >
        <p className="text-sm">{t('deleteConfirm')}</p>
        <TextInput label={t('currentPassword')} name="password" type="password" required />
      </FormDialog>
    </div>
  );
}

function NotificationSettings() {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const prefs = body(useGetNotificationPreferences().data) ?? [];
  const put = usePutNotificationPreference();
  const get = (c: string) => prefs.find((p) => p.channel === c);
  const set = async (channel: string, enabled: boolean, config?: Record<string, unknown>) => {
    await put.mutateAsync({ channel, data: { enabled, config: config ?? (get(channel)?.config as Record<string, unknown>) ?? {} } });
    await qc.invalidateQueries();
  };
  return (
    <Card>
      <Card.Header><Card.Title>{t('notifications')}</Card.Title></Card.Header>
      <Card.Content className="flex flex-col gap-3">
        {(['in_app', 'email'] as const).map((c) => (
          <Switch className="flex flex-row items-center gap-2" key={c} isSelected={get(c)?.enabled ?? c === 'in_app'} onChange={(v) => set(c, v)} data-testid={`pref-${c}`}>
            <Switch.Control><Switch.Thumb /></Switch.Control>
            <Switch.Content><Label>{t(`channel.${c}`)}</Label></Switch.Content>
          </Switch>
        ))}
      </Card.Content>
    </Card>
  );
}

function SlackSettings() {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const integrations = body(useListIntegrations().data);
  const slackEnabled = integrations?.enabled.includes('slack');
  const connected = integrations?.connected.find((c) => c.type === 'slack');
  const authUrl = useGetSlackAuthUrl({ query: { enabled: false } });
  const disconnect = useSlackDisconnect();
  const channels = body(useListSlackChannels({ query: { enabled: !!connected } }).data) ?? [];
  const prefs = body(useGetNotificationPreferences().data) ?? [];
  const slack = prefs.find((p) => p.channel === 'slack');
  const cfg = (slack?.config ?? {}) as { isDmEnabled?: boolean; channelId?: string };
  const put = usePutNotificationPreference();
  if (!slackEnabled) return null;
  const save = async (enabled: boolean, config: Record<string, unknown>) => {
    await put.mutateAsync({ channel: 'slack', data: { enabled, config } });
    await qc.invalidateQueries();
  };
  return (
    <Card>
      <Card.Header><Card.Title>Slack</Card.Title></Card.Header>
      <Card.Content className="flex flex-col gap-3">
        {!connected ? (
          <Button
            className="self-start"
            onPress={async () => {
              const r = await authUrl.refetch();
              const url = body(r.data)?.url;
              if (url) window.location.assign(url);
            }}
          >
            {t('connectSlack')}
          </Button>
        ) : (
          <>
            <p className="text-sm text-muted">{t('slackConnected')}</p>
            <Switch className="flex flex-row items-center gap-2" isSelected={slack?.enabled ?? false} onChange={(v) => save(v, cfg)}>
              <Switch.Control><Switch.Thumb /></Switch.Control>
              <Switch.Content><Label>{t('slackNotifications')}</Label></Switch.Content>
            </Switch>
            <Switch className="flex flex-row items-center gap-2" isSelected={cfg.isDmEnabled ?? false} onChange={(v) => save(slack?.enabled ?? true, { ...cfg, isDmEnabled: v })}>
              <Switch.Control><Switch.Thumb /></Switch.Control>
              <Switch.Content><Label>{t('slackDm')}</Label></Switch.Content>
            </Switch>
            <SimpleSelect label={t('slackChannel')} value={cfg.channelId ?? null} allowEmpty onChange={(v) => save(slack?.enabled ?? true, { ...cfg, channelId: v === '__none' ? '' : v })} options={channels.map((c) => ({ value: c.id, label: `#${c.name}` }))} />
            <Button variant="danger-soft" className="self-start" onPress={async () => { await disconnect.mutateAsync(); await qc.invalidateQueries(); }}>{t('disconnect')}</Button>
          </>
        )}
      </Card.Content>
    </Card>
  );
}
