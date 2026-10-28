'use client';

import {
  Bell,
  Cpu,
  KeyRound,
  MonitorSmartphone,
  Palette,
  UserRound,
} from 'lucide-react';
import { useTranslations } from 'next-intl';
import { notFound } from 'next/navigation';
import { useMe } from '../../api/generated/auth/auth';
import { useGetSystemInfo } from '../../api/generated/system/system';
import { body } from '../../lib/api';
import { fullName } from '../../lib/format';
import { AccountAppearance } from './account-appearance';
import { AccountNotifications } from './account-notifications';
import { AccountProfile } from './account-profile';
import { AccountSecurity } from './account-security';
import { ApiTokenSettings } from './api-token-settings';
import { McpSettings } from './mcp-settings';
import { SessionSettings } from './session-settings';
import { SettingsShell, type NavGroup } from './settings-shell';

const SECTIONS = [
  'profile',
  'appearance',
  'notifications',
  'security',
  'sessions',
  'api-tokens',
] as const;
export type AccountSection = (typeof SECTIONS)[number];

function accountSectionFrom(segment?: string): AccountSection {
  if (!segment) return 'profile';
  const hit = SECTIONS.find((s) => s === segment);
  if (!hit) notFound();
  return hit;
}

const ICON = {
  profile: UserRound,
  appearance: Palette,
  notifications: Bell,
  security: KeyRound,
  sessions: MonitorSmartphone,
  'api-tokens': Cpu,
} as const;

/** Everything about your own account, in the same frame as the project and workspace settings. */
export function AccountSettingsShell({ segment }: { segment?: string }) {
  const section = accountSectionFrom(segment);
  const t = useTranslations('asettings');
  const me = body(useMe().data);
  const info = body(useGetSystemInfo().data);
  const item = (s: AccountSection) => ({
    id: s,
    href: `/settings${s === 'profile' ? '' : `/${s}`}`,
    icon: ICON[s],
    label: t(`nav.${s}`),
    testId: `atab-${s}`,
  });
  const groups: NavGroup[] = [
    {
      label: t('group.account'),
      items: [item('profile'), item('appearance'), item('notifications')],
    },
    {
      label: t('group.security'),
      items: [item('security'), item('sessions'), item('api-tokens')],
    },
  ];

  return (
    <SettingsShell
      back={{ href: '/', label: t('back'), testId: 'back-home' }}
      identity={{
        initials: me
          ? `${me.first_name[0] ?? ''}${me.last_name[0] ?? ''}`.toUpperCase()
          : '··',
        name: me ? fullName(me) : '…',
        caption: t('title'),
      }}
      groups={groups}
      activeId={section}
      trail={`${me ? fullName(me) : ''} / ${t('title')}`}
      page={{
        icon: ICON[section],
        title: t(`page.${section}.title`),
        description: t(`page.${section}.description`),
      }}
      ready={!!me}
    >
      {me && section === 'profile' && <AccountProfile me={me} />}
      {me && section === 'appearance' && <AccountAppearance />}
      {section === 'notifications' && <AccountNotifications />}
      {me && section === 'security' && (
        <AccountSecurity
          me={me}
          providers={info?.sso_providers ?? []}
          mfaAvailable={info?.mfa_available ?? false}
        />
      )}
      {section === 'sessions' && <SessionSettings />}
      {section === 'api-tokens' && (
        <div className="flex flex-col gap-4">
          <ApiTokenSettings />
          <McpSettings />
        </div>
      )}
    </SettingsShell>
  );
}
