'use client';

import { Button, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { LogOut } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import {
  useListSessions,
  useRevokeOtherSessions,
  useRevokeSession,
} from '../../api/generated/auth/auth';
import { body, errorKind, serverMessage } from '../../lib/api';
import { deviceLabel } from '../../lib/device';
import { formatDate } from '../../lib/format';
import { Panel } from '../ui/settings';

/** Where the account is signed in right now, with a way to end each sign-in. */
export function SessionSettings() {
  const t = useTranslations('sessions');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const sessions = body(useListSessions().data) ?? [];
  const revoke = useRevokeSession();
  const revokeOthers = useRevokeOtherSessions();
  const others = sessions.filter((s) => !s.current);

  const run = async (action: () => Promise<unknown>) => {
    try {
      await action();
      await qc.invalidateQueries();
    } catch (e) {
      toast.danger(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  return (
    <Panel data-testid="session-settings">
      <Panel.Header className="flex-row items-center justify-between">
        <div>
          <Panel.Title>{t('title')}</Panel.Title>
          <Panel.Description>{t('description')}</Panel.Description>
        </div>
        <Button
          size="sm"
          variant="secondary"
          isDisabled={others.length === 0}
          onPress={() => {
            if (!window.confirm(t('confirmOthers'))) return;
            void run(() => revokeOthers.mutateAsync());
          }}
          data-testid="revoke-other-sessions"
        >
          {t('revokeOthers')}
        </Button>
      </Panel.Header>
      <Panel.Content>
        <ul className="divide-y divide-border" data-testid="session-list">
          {sessions.map((s) => (
            <li key={s.id} className="flex items-center gap-3 py-2 text-sm">
              <div className="min-w-0 flex-1">
                <p className="font-medium">
                  {deviceLabel(s.user_agent) || t('unknownDevice')}
                  {s.current && (
                    <span className="ml-2 text-xs text-accent">
                      {t('thisDevice')}
                    </span>
                  )}
                </p>
                <p className="text-xs text-muted">
                  {s.ip ? `${s.ip} · ` : ''}
                  {t('signedIn', {
                    date: formatDate(s.created_at, locale, true),
                  })}{' '}
                  ·{' '}
                  {t('lastActive', {
                    date: formatDate(s.last_active_at, locale, true),
                  })}
                </p>
              </div>
              {!s.current && (
                <Button
                  isIconOnly
                  size="sm"
                  variant="ghost"
                  aria-label={t('revoke')}
                  onPress={() =>
                    void run(() => revoke.mutateAsync({ id: s.id }))
                  }
                >
                  <LogOut size={14} />
                </Button>
              )}
            </li>
          ))}
        </ul>
      </Panel.Content>
    </Panel>
  );
}
