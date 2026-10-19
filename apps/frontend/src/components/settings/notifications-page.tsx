'use client';

import { Button } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useLocale, useTranslations } from 'next-intl';
import Link from 'next/link';
import { useListNotifications, useMarkAllNotificationsRead, useMarkNotificationRead } from '../../api/generated/notifications/notifications';
import { body } from '../../lib/api';
import { relativeTime } from '../../lib/format';

export function NotificationsPage() {
  const t = useTranslations('notifications');
  const locale = useLocale();
  const qc = useQueryClient();
  const items = body(useListNotifications({ limit: 100 }).data) ?? [];
  const markAll = useMarkAllNotificationsRead();
  const markOne = useMarkNotificationRead();
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t('title')}</h1>
        <Button variant="tertiary" size="sm" onPress={async () => { await markAll.mutateAsync(); await qc.invalidateQueries(); }} data-testid="mark-all">
          {t('markAll')}
        </Button>
      </div>
      {items.length === 0 && <p className="text-muted">{t('empty')}</p>}
      <ul className="divide-y divide-separator rounded-lg border border-separator" data-testid="notification-list">
        {items.map((n) => {
          const href = n.related_resource_type === 'card' && n.related_resource_id && n.workspace_id ? `/workspaces/${n.workspace_id}/cards/${n.related_resource_id}` : null;
          const text = (
            <>
              <span className={n.is_read ? 'text-muted' : 'font-medium'}>{n.message}</span>
              <span className="ml-2 text-xs text-muted">{relativeTime(n.created_at, locale)}</span>
            </>
          );
          return (
            <li key={n.id} className="flex items-center justify-between gap-2 px-3 py-2 text-sm">
              {href ? (
                <Link
                  href={href}
                  onClick={async () => {
                    if (!n.is_read) await markOne.mutateAsync({ id: n.id }).catch(() => undefined);
                    await qc.invalidateQueries();
                  }}
                >
                  {text}
                </Link>
              ) : (
                <span>{text}</span>
              )}
              {!n.is_read && <span aria-label={t('unread')} className="size-2 rounded-full bg-accent" />}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
