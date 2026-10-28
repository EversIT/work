'use client';

import {
  Badge,
  Button,
  Dropdown,
  Tooltip,
  buttonVariants,
  toast,
} from '@heroui/react';
import { routes } from '../../lib/routes';
import { useQueryClient } from '@tanstack/react-query';
import { Bell, ChevronDown, Menu, Plus, Search, Settings } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useLogout } from '../../api/generated/auth/auth';
import { useListNotifications } from '../../api/generated/notifications/notifications';
import { body } from '../../lib/api';
import { useServerEvents } from '../../lib/events';
import { fullName } from '../../lib/format';
import { useCurrentUser } from '../../lib/hooks';
import { useCreateIssue } from './create-issue-context';
import { SearchBox } from './search-box';
import { Brand, useBranding } from '../brand';
import { UserAvatar } from './user-avatar';
import { useCreateProject } from './create-project-context';
import { useOptionalWorkspaceCtx } from './workspace-context';
import { useAllWorkspaces } from '../../lib/hooks';
import { readRecentProjects } from '../../lib/recent';

type ListLink = { slug: string; name: string; spaceName: string };

/** Recently opened projects of this workspace first, then the rest, at most six. */
function recent(ctx: { workspaceId: string; lists: ListLink[] }): ListLink[] {
  const seen = readRecentProjects(ctx.workspaceId);
  const byRecency = seen
    .map((slug) => ctx.lists.find((l) => l.slug === slug))
    .filter((l): l is ListLink => !!l);
  return [
    ...byRecency,
    ...ctx.lists.filter((l) => !seen.includes(l.slug)),
  ].slice(0, 6);
}

export function TopNav({
  onMenu,
  showMenu,
}: {
  onMenu: () => void;
  showMenu: boolean;
}) {
  const t = useTranslations('nav');
  const router = useRouter();
  const qc = useQueryClient();
  const branding = useBranding();
  const me = useCurrentUser();
  const ctx = useOptionalWorkspaceCtx();
  const logout = useLogout();
  const create = useCreateIssue();
  const createProject = useCreateProject();
  const { workspaces } = useAllWorkspaces();
  const [searchOpen, setSearchOpen] = useState(false);
  const unread =
    body(
      useListNotifications(
        { is_read: false, limit: 50 },
        { query: { refetchInterval: 30_000 } },
      ).data,
    ) ?? [];

  useServerEvents((e) => {
    qc.invalidateQueries({ queryKey: [`/api/v1/notifications`] });
    toast(e.message, { timeout: 6000 });
  });

  const wsKey = ctx?.workspace?.key_prefix;
  const home = wsKey ? routes.workspace(wsKey) : '/';
  const link = `${buttonVariants({ variant: 'ghost', size: 'sm' })} inline-flex flex-row items-center gap-1`;
  const iconLink = buttonVariants({
    variant: 'ghost',
    size: 'sm',
    isIconOnly: true,
  });

  return (
    <>
      <header
        className="flex h-14 shrink-0 items-center gap-1 border-b border-border bg-surface px-3"
        data-testid="top-nav"
      >
        {showMenu && (
          <Button
            isIconOnly
            variant="ghost"
            size="sm"
            className="md:hidden"
            aria-label={t('menu')}
            onPress={onMenu}
          >
            <Menu size={18} />
          </Button>
        )}
        <Link
          href="/"
          className="mr-2 flex items-center gap-2 px-1 text-foreground"
          aria-label={branding.title}
        >
          <Brand hideNameOnMobile />
        </Link>
        {ctx && workspaces.length > 1 && (
          <Dropdown>
            <Dropdown.Trigger
              className={`${link} max-w-40`}
              data-testid="workspace-switcher"
              aria-label={t('chooseWorkspace')}
            >
              <span className="truncate">
                {ctx.workspace?.name ?? t('chooseWorkspace')}
              </span>
              <ChevronDown size={14} />
            </Dropdown.Trigger>
            <Dropdown.Popover>
              <Dropdown.Menu
                aria-label={t('workspaces')}
                onAction={(key) => {
                  const w = workspaces.find((x) => x.id === String(key));
                  if (w) router.push(routes.workspace(w.key_prefix));
                }}
              >
                {workspaces.map((w) => (
                  <Dropdown.Item key={w.id} id={w.id} textValue={w.name}>
                    <span className="flex flex-col">
                      <span>{w.name}</span>
                      <span className="text-xs text-muted">
                        {w.project.name}
                      </span>
                    </span>
                  </Dropdown.Item>
                ))}
              </Dropdown.Menu>
            </Dropdown.Popover>
          </Dropdown>
        )}
        <nav
          className="hidden items-center gap-0.5 md:flex"
          aria-label={t('main')}
        >
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
                <Dropdown.Menu
                  aria-label={t('projects')}
                  onAction={(key) => {
                    const k = String(key);
                    if (k === 'create') createProject.open();
                    else
                      router.push(k === 'all' ? home : k.replace(/^go:/, ''));
                  }}
                >
                  <Dropdown.Section aria-label={t('recent')}>
                    {recent(ctx).map((l) => (
                      <Dropdown.Item
                        key={l.slug}
                        id={`go:${routes.project(wsKey ?? '', l.slug)}`}
                        textValue={l.name}
                      >
                        <span className="flex flex-col">
                          <span>{l.name}</span>
                          <span className="text-xs text-muted">
                            {l.spaceName}
                          </span>
                        </span>
                      </Dropdown.Item>
                    ))}
                  </Dropdown.Section>
                  <Dropdown.Item id="all" textValue={t('viewAllProjects')}>
                    {t('viewAllProjects')}
                  </Dropdown.Item>
                  <Dropdown.Item id="create" textValue={t('createProject')}>
                    {t('createProject')}
                  </Dropdown.Item>
                </Dropdown.Menu>
              </Dropdown.Popover>
            </Dropdown>
          )}
        </nav>
        {ctx && (
          <Button
            size="sm"
            className="ml-1 md:ml-2"
            aria-label={t('create')}
            onPress={() => create.open()}
            data-testid="create-button"
            isDisabled={ctx.lists.length === 0}
          >
            <Plus size={14} />
            <span className="hidden sm:inline">{t('create')}</span>
          </Button>
        )}
        <div className="flex-1" />
        {ctx && (
          <div className="hidden w-full max-w-xs md:block">
            <SearchBox />
          </div>
        )}
        {ctx && (
          <Button
            isIconOnly
            variant="ghost"
            size="sm"
            className="md:hidden"
            aria-label={t('search')}
            aria-expanded={searchOpen}
            onPress={() => setSearchOpen((o) => !o)}
          >
            <Search size={18} />
          </Button>
        )}
        <Tooltip>
          <Tooltip.Trigger>
            <Badge.Anchor>
              <Link
                href="/notifications"
                aria-label={t('notifications')}
                className={iconLink}
                data-testid="bell"
              >
                <Bell size={18} />
              </Link>
              {unread.length > 0 && (
                <Badge color="danger" size="sm" data-testid="unread-count">
                  {unread.length > 9 ? '9+' : unread.length}
                </Badge>
              )}
            </Badge.Anchor>
          </Tooltip.Trigger>
          <Tooltip.Content>{t('notifications')}</Tooltip.Content>
        </Tooltip>
        {ctx && (
          <Tooltip>
            <Tooltip.Trigger>
              <Link
                href={routes.workspaceSettings(ctx.workspace?.key_prefix ?? '')}
                aria-label={t('workspaceSettings')}
                className={`${iconLink} hidden md:inline-flex`}
                data-testid="workspace-settings-link"
              >
                <Settings size={18} />
              </Link>
            </Tooltip.Trigger>
            <Tooltip.Content>{t('workspaceSettings')}</Tooltip.Content>
          </Tooltip>
        )}
        {me && (
          <Dropdown>
            <Dropdown.Trigger
              className={buttonVariants({
                variant: 'ghost',
                isIconOnly: true,
                size: 'sm',
              })}
              aria-label={t('account')}
              data-testid="user-menu"
            >
              <UserAvatar user={me} />
            </Dropdown.Trigger>
            <Dropdown.Popover>
              <Dropdown.Menu
                aria-label={t('account')}
                onAction={async (key) => {
                  if (key === 'settings') router.push('/settings');
                  if (key === 'admin') router.push('/admin');
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
                {me.is_instance_admin ? (
                  <Dropdown.Item id="admin" textValue={t('admin')}>
                    {t('admin')}
                  </Dropdown.Item>
                ) : null}
                <Dropdown.Item
                  id="signout"
                  textValue={t('signOut')}
                  variant="danger"
                >
                  {t('signOut')}
                </Dropdown.Item>
              </Dropdown.Menu>
            </Dropdown.Popover>
          </Dropdown>
        )}
      </header>
      {ctx && searchOpen && (
        <div className="border-b border-border bg-surface p-2 md:hidden">
          <SearchBox className="max-w-none" />
        </div>
      )}
    </>
  );
}
