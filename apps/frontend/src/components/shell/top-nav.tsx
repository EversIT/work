'use client';

import { Button, Dropdown, Tooltip, buttonVariants, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Bell, ChevronDown, Menu, Plus, Settings } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useLogout } from '../../api/generated/auth/auth';
import { useListNotifications } from '../../api/generated/notifications/notifications';
import { body } from '../../lib/api';
import { useServerEvents } from '../../lib/events';
import { fullName } from '../../lib/format';
import { useCurrentUser } from '../../lib/hooks';
import { useCreateIssue } from './create-issue-context';
import { SearchBox } from './search-box';
import { UserAvatar } from './user-avatar';
import { useOptionalWorkspaceCtx } from './workspace-context';

export function TopNav({ onMenu }: { onMenu: () => void }) {
  const t = useTranslations('nav');
  const router = useRouter();
  const qc = useQueryClient();
  const me = useCurrentUser();
  const ctx = useOptionalWorkspaceCtx();
  const logout = useLogout();
  const create = useCreateIssue();
  const unread = body(useListNotifications({ is_read: false, limit: 50 }, { query: { refetchInterval: 30_000 } }).data) ?? [];

  useServerEvents((e) => {
    qc.invalidateQueries({ queryKey: [`/api/v1/notifications`] });
    toast(e.message, { timeout: 6000 });
  });

  const home = ctx ? `/workspaces/${ctx.workspaceId}` : '/';
  const link = 'inline-flex h-8 items-center gap-1 rounded-[3px] px-2.5 text-sm font-medium text-[#44546F] hover:bg-[#091E420F]';

  return (
    <header className="flex h-14 shrink-0 items-center gap-1 border-b border-[#DCDFE4] bg-white px-3" data-testid="top-nav">
      <Button isIconOnly variant="ghost" size="sm" className="md:hidden" aria-label={t('menu')} onPress={onMenu}>
        <Menu size={18} />
      </Button>
      <Link href="/" className="mr-2 flex items-center gap-2 px-1" aria-label="work">
        <span className="flex size-7 items-center justify-center rounded-[4px] bg-gradient-to-br from-[#0C66E4] to-[#1D7AFC] text-sm font-bold text-white">w</span>
        <span className="hidden text-base font-semibold text-[#172B4D] sm:inline">work</span>
      </Link>
      <nav className="hidden items-center gap-0.5 md:flex" aria-label={t('main')}>
        <Link href={home} className={link}>
          {t('yourWork')}
        </Link>
        {ctx && (
          <Dropdown>
            <Dropdown.Trigger className={link} data-testid="projects-menu">
              {t('projects')}
              <ChevronDown size={14} />
            </Dropdown.Trigger>
            <Dropdown.Popover>
              <Dropdown.Menu aria-label={t('projects')} onAction={(key) => router.push(String(key))}>
                {ctx.lists.length === 0 ? (
                  <Dropdown.Item id={home} textValue={t('noProjects')}>
                    {t('noProjects')}
                  </Dropdown.Item>
                ) : (
                  ctx.lists.map((l) => (
                    <Dropdown.Item key={l.id} id={`/workspaces/${ctx.workspaceId}/lists/${l.slug}`} textValue={l.name}>
                      <span className="flex flex-col">
                        <span>{l.name}</span>
                        <span className="text-xs text-muted">{l.spaceName}</span>
                      </span>
                    </Dropdown.Item>
                  ))
                )}
              </Dropdown.Menu>
            </Dropdown.Popover>
          </Dropdown>
        )}
      </nav>
      {ctx && (
        <Button size="sm" className="ml-2 rounded-[3px]" onPress={() => create.open()} data-testid="create-button" isDisabled={ctx.lists.length === 0}>
          <Plus size={14} />
          {t('create')}
        </Button>
      )}
      <div className="flex-1" />
      {ctx && <SearchBox />}
      <Tooltip>
        <Tooltip.Trigger>
          <Link href="/notifications" aria-label={t('notifications')} className="relative ml-1 inline-flex size-8 items-center justify-center rounded-full text-[#44546F] hover:bg-[#091E420F]" data-testid="bell">
            <Bell size={18} />
            {unread.length > 0 && (
              <span className="absolute -right-0.5 -top-0.5 flex size-4 items-center justify-center rounded-full bg-[#C9372C] text-[10px] font-bold text-white" data-testid="unread-count">
                {unread.length > 9 ? '9+' : unread.length}
              </span>
            )}
          </Link>
        </Tooltip.Trigger>
        <Tooltip.Content>{t('notifications')}</Tooltip.Content>
      </Tooltip>
      {ctx && (
        <Tooltip>
          <Tooltip.Trigger>
            <Link href={`/workspaces/${ctx.workspaceId}/settings`} aria-label={t('workspaceSettings')} className="inline-flex size-8 items-center justify-center rounded-full text-[#44546F] hover:bg-[#091E420F]" data-testid="workspace-settings-link">
              <Settings size={18} />
            </Link>
          </Tooltip.Trigger>
          <Tooltip.Content>{t('workspaceSettings')}</Tooltip.Content>
        </Tooltip>
      )}
      {me && (
        <Dropdown>
          <Dropdown.Trigger className={buttonVariants({ variant: 'ghost', isIconOnly: true, size: 'sm' })} aria-label={t('account')} data-testid="user-menu">
            <UserAvatar user={me} />
          </Dropdown.Trigger>
          <Dropdown.Popover>
            <Dropdown.Menu
              aria-label={t('account')}
              onAction={async (key) => {
                if (key === 'settings') router.push('/settings');
                if (key === 'signout') {
                  await logout.mutateAsync().catch(() => undefined);
                  qc.clear();
                  router.replace('/login');
                }
              }}
            >
              <Dropdown.Item id="name" textValue={fullName(me)} isDisabled>
                {fullName(me)}
              </Dropdown.Item>
              <Dropdown.Item id="settings" textValue={t('settings')}>
                {t('settings')}
              </Dropdown.Item>
              <Dropdown.Item id="signout" textValue={t('signOut')} variant="danger">
                {t('signOut')}
              </Dropdown.Item>
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
      )}
    </header>
  );
}
