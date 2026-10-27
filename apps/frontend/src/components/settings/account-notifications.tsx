'use client';

import { Button, Label, Switch } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import {
  useGetSlackAuthUrl,
  useListIntegrations,
  useListSlackChannels,
  useSlackDisconnect,
} from '../../api/generated/integrations/integrations';
import {
  useGetNotificationPreferences,
  usePutNotificationPreference,
} from '../../api/generated/notifications/notifications';
import { body } from '../../lib/api';
import { SimpleSelect } from '../ui/inputs';
import { Panel } from '../ui/settings';

/** How you hear about changes: in the app, by e-mail and, if connected, in Slack. */
export function AccountNotifications() {
  return (
    <div className="flex flex-col gap-6">
      <NotificationSettings />
      <SlackSettings />
    </div>
  );
}

function NotificationSettings() {
  const t = useTranslations('settings');
  const qc = useQueryClient();
  const prefs = body(useGetNotificationPreferences().data) ?? [];
  const put = usePutNotificationPreference();
  const get = (c: string) => prefs.find((p) => p.channel === c);
  const set = async (
    channel: string,
    enabled: boolean,
    config?: Record<string, unknown>,
  ) => {
    await put.mutateAsync({
      channel,
      data: {
        enabled,
        config:
          config ?? (get(channel)?.config as Record<string, unknown>) ?? {},
      },
    });
    await qc.invalidateQueries();
  };
  return (
    <Panel>
      <Panel.Header>
        <Panel.Title>{t('notifications')}</Panel.Title>
      </Panel.Header>
      <Panel.Content className="flex flex-col gap-3">
        {(['in_app', 'email'] as const).map((c) => (
          <Switch
            className="flex flex-row items-center gap-2"
            key={c}
            isSelected={get(c)?.enabled ?? c === 'in_app'}
            onChange={(v) => set(c, v)}
            data-testid={`pref-${c}`}
          >
            <Switch.Control>
              <Switch.Thumb />
            </Switch.Control>
            <Switch.Content>
              <Label>{t(`channel.${c}`)}</Label>
            </Switch.Content>
          </Switch>
        ))}
      </Panel.Content>
    </Panel>
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
  const channels =
    body(useListSlackChannels({ query: { enabled: !!connected } }).data) ?? [];
  const prefs = body(useGetNotificationPreferences().data) ?? [];
  const slack = prefs.find((p) => p.channel === 'slack');
  const cfg = (slack?.config ?? {}) as {
    isDmEnabled?: boolean;
    channelId?: string;
  };
  const put = usePutNotificationPreference();
  if (!slackEnabled) return null;
  const save = async (enabled: boolean, config: Record<string, unknown>) => {
    await put.mutateAsync({ channel: 'slack', data: { enabled, config } });
    await qc.invalidateQueries();
  };
  return (
    <Panel>
      <Panel.Header>
        <Panel.Title>Slack</Panel.Title>
      </Panel.Header>
      <Panel.Content className="flex flex-col gap-3">
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
            <Switch
              className="flex flex-row items-center gap-2"
              isSelected={slack?.enabled ?? false}
              onChange={(v) => save(v, cfg)}
            >
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
              <Switch.Content>
                <Label>{t('slackNotifications')}</Label>
              </Switch.Content>
            </Switch>
            <Switch
              className="flex flex-row items-center gap-2"
              isSelected={cfg.isDmEnabled ?? false}
              onChange={(v) =>
                save(slack?.enabled ?? true, { ...cfg, isDmEnabled: v })
              }
            >
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
              <Switch.Content>
                <Label>{t('slackDm')}</Label>
              </Switch.Content>
            </Switch>
            <SimpleSelect
              label={t('slackChannel')}
              value={cfg.channelId ?? null}
              allowEmpty
              onChange={(v) =>
                save(slack?.enabled ?? true, {
                  ...cfg,
                  channelId: v === '__none' ? '' : v,
                })
              }
              options={channels.map((c) => ({
                value: c.id,
                label: `#${c.name}`,
              }))}
            />
            <Button
              variant="danger-soft"
              className="self-start"
              onPress={async () => {
                await disconnect.mutateAsync();
                await qc.invalidateQueries();
              }}
            >
              {t('disconnect')}
            </Button>
          </>
        )}
      </Panel.Content>
    </Panel>
  );
}
