'use client';

import { Card, Chip } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useAdminOverview } from '../../api/generated/admin/admin';
import { body } from '../../lib/api';

const bytes = (n: number) =>
  n < 1024 ** 2
    ? `${(n / 1024).toFixed(0)} KB`
    : n < 1024 ** 3
      ? `${(n / 1024 ** 2).toFixed(1)} MB`
      : `${(n / 1024 ** 3).toFixed(2)} GB`;

/** Counts of everything on the instance, and how it is configured (never a secret). */
export function Overview() {
  const t = useTranslations('admin');
  const o = body(useAdminOverview().data);
  if (!o) return null;
  const c = o.counts;
  const stats: [string, string | number][] = [
    ['users', c.users],
    ['admins', c.admins],
    ['disabledUsers', c.users_disabled],
    ['usersWithMfa', c.users_with_mfa],
    ['usersWithPasskeys', c.users_with_passkeys],
    ['projects', c.projects],
    ['workspaces', c.workspaces],
    ['lists', c.lists],
    ['cards', c.cards],
    ['comments', c.comments],
    ['files', `${c.files} · ${bytes(c.file_bytes)}`],
    ['automations', c.automations_enabled],
    ['sessions', c.active_sessions],
    ['apiTokens', c.api_tokens],
  ];
  const cfg = o.config;
  const flag = (on: boolean) => (
    <Chip size="sm" color={on ? 'success' : 'default'}>
      <Chip.Label>{on ? t('on') : t('off')}</Chip.Label>
    </Chip>
  );
  return (
    <div className="flex flex-col gap-6">
      <div
        className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5"
        data-testid="admin-stats"
      >
        {stats.map(([key, value]) => (
          <Card key={key}>
            <Card.Content>
              <p className="text-xs uppercase text-muted">{t(`stat.${key}`)}</p>
              <p className="text-2xl font-semibold" data-testid={`stat-${key}`}>
                {value}
              </p>
            </Card.Content>
          </Card>
        ))}
      </div>
      <Card>
        <Card.Header>
          <Card.Title>{t('configuration')}</Card.Title>
          <Card.Description>{t('configurationHint')}</Card.Description>
        </Card.Header>
        <Card.Content>
          <dl className="grid gap-x-8 gap-y-2 text-sm sm:grid-cols-2">
            <Row label={t('cfg.version')}>{cfg.version}</Row>
            <Row label={t('cfg.registration')}>
              {flag(cfg.registration_enabled)}
            </Row>
            <Row label={t('cfg.email')}>{flag(cfg.email_enabled)}</Row>
            <Row label={t('cfg.emailVerification')}>
              {flag(cfg.email_verification_required)}
            </Row>
            <Row label={t('cfg.mfa')}>{flag(cfg.mfa_available)}</Row>
            <Row label={t('cfg.sso')}>
              {cfg.sso_providers.length ? cfg.sso_providers.join(', ') : '—'}
            </Row>
            <Row label={t('cfg.storage')}>{cfg.storage}</Row>
            <Row label={t('cfg.scheduler')}>{flag(cfg.scheduler)}</Row>
            <Row label={t('cfg.sharedRateLimit')}>
              {flag(cfg.shared_rate_limit)}
            </Row>
          </dl>
        </Card.Content>
      </Card>
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border py-1.5">
      <dt className="text-muted">{label}</dt>
      <dd className="font-medium">{children}</dd>
    </div>
  );
}
