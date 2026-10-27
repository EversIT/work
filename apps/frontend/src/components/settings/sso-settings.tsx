'use client';

import { Button, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Panel } from '../ui/settings';
import { useTranslations } from 'next-intl';
import { useSearchParams } from 'next/navigation';
import { useEffect, useRef } from 'react';
import {
  useListSsoLinks,
  useStartSsoLink,
  useUnlinkSso,
} from '../../api/generated/auth/auth';
import { body, errorKind, serverMessage } from '../../lib/api';

/** Connect or disconnect Google, GitHub, Microsoft, Slack or SAML sign-in for this account. */
export function SsoSettings({ providers }: { providers: string[] }) {
  const t = useTranslations('sso');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const links = body(useListSsoLinks().data) ?? [];
  const start = useStartSsoLink();
  const unlink = useUnlinkSso();
  const search = useSearchParams();
  const announced = useRef(false);

  useEffect(() => {
    if (search.get('sso') === 'linked' && !announced.current) {
      announced.current = true;
      toast.success(t('linked'));
    }
  }, [search, t]);

  const all = Array.from(
    new Set([...providers, ...links.map((l) => l.provider)]),
  );
  if (all.length === 0) return null;
  return (
    <Panel data-testid="sso-settings">
      <Panel.Header>
        <div>
          <Panel.Title>{t('title')}</Panel.Title>
          <Panel.Description>{t('description')}</Panel.Description>
        </div>
      </Panel.Header>
      <Panel.Content className="flex flex-col gap-2">
        {all.map((p) => {
          const link = links.find((l) => l.provider === p);
          return (
            <div
              key={p}
              className="flex items-center justify-between gap-3 text-sm"
              data-testid={`sso-row-${p}`}
            >
              <div>
                <p className="font-medium">{t(`providers.${p}`)}</p>
                <p className="text-xs text-muted">
                  {link ? (link.email ?? t('connected')) : t('notConnected')}
                </p>
              </div>
              {link ? (
                <Button
                  size="sm"
                  variant="danger-soft"
                  isPending={unlink.isPending}
                  onPress={async () => {
                    try {
                      await unlink.mutateAsync({ provider: p });
                      await qc.invalidateQueries();
                    } catch (e) {
                      toast.danger(
                        errorKind(e) === 'conflict'
                          ? t('lastWayIn')
                          : (serverMessage(e) ?? te(errorKind(e))),
                      );
                    }
                  }}
                >
                  {t('disconnect')}
                </Button>
              ) : (
                <Button
                  size="sm"
                  isPending={start.isPending}
                  onPress={async () => {
                    try {
                      const url = body(
                        await start.mutateAsync({ provider: p }),
                      )?.url;
                      if (url) window.location.assign(url);
                    } catch (e) {
                      toast.danger(serverMessage(e) ?? te(errorKind(e)));
                    }
                  }}
                >
                  {t('connect')}
                </Button>
              )}
            </div>
          );
        })}
      </Panel.Content>
    </Panel>
  );
}
