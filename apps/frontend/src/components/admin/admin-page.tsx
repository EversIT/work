'use client';

import { Tabs } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useCurrentUser } from '../../lib/hooks';
import { AiSettings } from './ai-settings';
import { Appearance } from './appearance';
import { AuditLog } from './audit-log';
import { Overview } from './overview';
import { Projects } from './projects';
import { Users } from './users';

/** The instance administration. The backend answers 404 for everybody who is not an administrator. */
export function AdminPage() {
  const t = useTranslations('admin');
  const me = useCurrentUser();
  if (!me) return null;
  if (!me.is_instance_admin) {
    return (
      <p className="p-8 text-muted" data-testid="admin-denied">
        {t('denied')}
      </p>
    );
  }
  return (
    <div
      className="mx-auto flex max-w-6xl flex-col gap-4 p-6"
      data-testid="admin-page"
    >
      <h1 className="text-2xl font-medium text-foreground">{t('title')}</h1>
      <Tabs>
        <Tabs.ListContainer>
          <Tabs.List aria-label={t('title')}>
            {(
              [
                'overview',
                'users',
                'projects',
                'ai',
                'appearance',
                'audit',
              ] as const
            ).map((id) => (
              <Tabs.Tab key={id} id={id} data-testid={`admin-tab-${id}`}>
                {t(`tab.${id}`)}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel id="overview" className="pt-4">
          <Overview />
        </Tabs.Panel>
        <Tabs.Panel id="users" className="pt-4">
          <Users meId={me.id} />
        </Tabs.Panel>
        <Tabs.Panel id="projects" className="pt-4">
          <Projects />
        </Tabs.Panel>
        <Tabs.Panel id="ai" className="pt-4">
          <AiSettings />
        </Tabs.Panel>
        <Tabs.Panel id="appearance" className="pt-4">
          <Appearance />
        </Tabs.Panel>
        <Tabs.Panel id="audit" className="pt-4">
          <AuditLog />
        </Tabs.Panel>
      </Tabs>
    </div>
  );
}
