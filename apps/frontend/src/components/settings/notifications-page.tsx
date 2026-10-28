'use client';

import { Button } from '@heroui/react';
import { Sparkles } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import { useLocale, useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { getCard } from '../../api/generated/cards/cards';
import {
  useListNotifications,
  useMarkAllNotificationsRead,
  useMarkNotificationRead,
} from '../../api/generated/notifications/notifications';
import { body } from '../../lib/api';
import { relativeTime } from '../../lib/format';
import { digest } from '../../lib/notification-digest';
import { routes } from '../../lib/routes';
import { useAiStatus } from '../../lib/ai';

const DIGEST_TYPES = {
  mention: 1,
  assigned: 1,
  unassigned: 1,
  comment: 1,
  stage_updated: 1,
  subscription: 1,
  automation: 1,
  system_announcement: 1,
};

export function NotificationsPage() {
  const t = useTranslations('notifications');
  const locale = useLocale();
  const qc = useQueryClient();
  const router = useRouter();
  const items = body(useListNotifications({ limit: 100 }).data) ?? [];
  const markAll = useMarkAllNotificationsRead();
  const markOne = useMarkNotificationRead();
  const ai = useAiStatus();
  const groups = digest(items);
  const openCard = async (cardId: string) => {
    // Notifications know the issue by id; the address uses its key.
    const card = body(await getCard(cardId).catch(() => undefined));
    if (card) router.push(routes.browse(card.key));
  };
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t('title')}</h1>
        <Button
          variant="tertiary"
          size="sm"
          onPress={async () => {
            await markAll.mutateAsync();
            await qc.invalidateQueries();
          }}
          data-testid="mark-all"
        >
          {t('markAll')}
        </Button>
      </div>
      {ai && groups.length > 0 && (
        <section
          aria-label={t('digestTitle')}
          className="rounded-lg border border-separator bg-surface-secondary p-3"
          data-testid="notification-digest"
        >
          <h2 className="flex items-center gap-1.5 text-sm font-semibold">
            <Sparkles size={14} aria-hidden />
            {t('digestTitle')}
          </h2>
          <p className="mb-2 text-xs text-muted">{t('digestHelp')}</p>
          <ul className="flex flex-col divide-y divide-separator">
            {groups.map((g) => {
              const others = Object.entries(g.counts)
                .map(
                  ([type, count]) =>
                    [type, count - (type === g.lead.type ? 1 : 0)] as const,
                )
                .filter(([, count]) => count > 0);
              return (
                <li
                  key={g.cardId ?? 'other'}
                  className="flex items-start gap-2 py-2 text-sm"
                >
                  <div className="min-w-0 flex-1">
                    <p className="font-medium">{g.lead.message}</p>
                    {others.length > 0 && (
                      <p className="text-xs text-muted">
                        {t('digestMore', {
                          count: g.notifications.length - 1,
                          types: others
                            .map(([type, count]) =>
                              t(
                                `digestType.${type in DIGEST_TYPES ? type : 'other'}`,
                                { count },
                              ),
                            )
                            .join(', '),
                        })}
                      </p>
                    )}
                  </div>
                  {g.cardId && (
                    <Button
                      size="sm"
                      variant="secondary"
                      onPress={() => g.cardId && openCard(g.cardId)}
                    >
                      {t('digestOpen')}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    data-testid="digest-read"
                    onPress={async () => {
                      await Promise.all(
                        g.notifications.map((n) =>
                          markOne
                            .mutateAsync({ id: n.id })
                            .catch(() => undefined),
                        ),
                      );
                      await qc.invalidateQueries();
                    }}
                  >
                    {t('digestRead')}
                  </Button>
                </li>
              );
            })}
          </ul>
        </section>
      )}
      {items.length === 0 && <p className="text-muted">{t('empty')}</p>}
      <ul
        className="divide-y divide-separator rounded-lg border border-separator"
        data-testid="notification-list"
      >
        {items.map((n) => {
          const cardId =
            n.related_resource_type === 'card' ? n.related_resource_id : null;
          const text = (
            <>
              <span className={n.is_read ? 'text-muted' : 'font-medium'}>
                {n.message}
              </span>
              <span className="ml-2 text-xs text-muted">
                {relativeTime(n.created_at, locale)}
              </span>
            </>
          );
          return (
            <li
              key={n.id}
              className="flex items-center justify-between gap-2 px-3 py-2 text-sm"
            >
              {cardId ? (
                <button
                  type="button"
                  className="text-left"
                  onClick={async () => {
                    if (!n.is_read)
                      await markOne
                        .mutateAsync({ id: n.id })
                        .catch(() => undefined);
                    await qc.invalidateQueries();
                    // Notifications know the issue by id; the address uses its key.
                    const card = body(
                      await getCard(cardId).catch(() => undefined),
                    );
                    if (card) router.push(routes.browse(card.key));
                  }}
                >
                  {text}
                </button>
              ) : (
                <span>{text}</span>
              )}
              {!n.is_read && (
                <span
                  aria-label={t('unread')}
                  className="size-2 rounded-full bg-accent"
                />
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
