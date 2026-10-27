'use client';

import { ArrowLeft, type LucideIcon } from 'lucide-react';
import Link from 'next/link';
import { PageHeader, SettingsSkeleton } from '../ui/settings';

export type NavItem = {
  id: string;
  href: string;
  icon: LucideIcon;
  label: string;
  testId: string;
};
export type NavGroup = { label: string; items: NavItem[] };

function NavLink({
  item,
  active,
  suffix = '',
}: {
  item: NavItem;
  active: boolean;
  suffix?: string;
}) {
  const Icon = item.icon;
  return (
    <Link
      href={item.href}
      data-testid={`${item.testId}${suffix}`}
      aria-current={active ? 'page' : undefined}
      className={`group relative flex h-9 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm transition-colors duration-150 before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:origin-center before:scale-y-0 before:rounded-full before:bg-accent before:transition-transform before:duration-200 aria-[current=page]:before:scale-y-100 ${
        active
          ? 'bg-accent-soft font-medium text-accent'
          : 'text-muted hover:bg-default hover:text-foreground'
      }`}
    >
      <Icon
        size={16}
        className="transition-transform duration-150 group-hover:scale-110"
      />
      <span className="truncate">{item.label}</span>
    </Link>
  );
}

/**
 * The frame of every settings area (project, workspace, account), as in Jira Service Management:
 * a sidebar of grouped sections on the left, and on the right a page that says what it is for.
 * Choosing a section re-plays a short entrance, so the change reads as navigation.
 */
export function SettingsShell({
  back,
  identity,
  groups,
  activeId,
  trail,
  page,
  ready = true,
  children,
}: {
  back: { href: string; label: string; testId: string };
  identity: { initials: string; name: string; caption: string };
  groups: NavGroup[];
  activeId: string;
  /** Breadcrumb above the page title. */
  trail: string;
  page: { icon: LucideIcon; title: string; description: string };
  ready?: boolean;
  children: React.ReactNode;
}) {
  const all = groups.flatMap((g) => g.items);
  return (
    <div className="flex min-h-[calc(100vh-3.5rem)]">
      <aside className="sticky top-0 hidden h-[calc(100vh-3.5rem)] w-64 shrink-0 flex-col gap-3 overflow-y-auto border-r border-separator bg-surface-secondary p-3 md:flex">
        <Link
          href={back.href}
          data-testid={back.testId}
          className="flex h-8 items-center gap-2 rounded-md px-2 text-xs font-medium text-muted transition-colors hover:bg-default hover:text-foreground"
        >
          <ArrowLeft size={14} />
          {back.label}
        </Link>
        <div className="flex items-center gap-2.5 px-1">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-accent-hover to-accent text-sm font-bold text-accent-foreground">
            {identity.initials}
          </span>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold text-foreground">
              {identity.name}
            </p>
            <p className="text-xs text-muted">{identity.caption}</p>
          </div>
        </div>
        <nav aria-label={identity.caption} className="flex flex-col gap-4 pt-1">
          {groups.map((g) => (
            <div key={g.label} className="flex flex-col gap-0.5">
              <p className="jira-eyebrow px-3 pb-1">{g.label}</p>
              {g.items.map((item) => (
                <NavLink
                  key={item.id}
                  item={item}
                  active={item.id === activeId}
                />
              ))}
            </div>
          ))}
        </nav>
      </aside>

      <main className="min-w-0 flex-1 px-4 py-6 md:px-10 md:py-8">
        {/* Small screens: the same sections as a strip that scrolls sideways. */}
        <nav
          aria-label={identity.caption}
          className="-mx-4 mb-5 flex gap-1 overflow-x-auto px-4 pb-1 md:hidden"
        >
          {all.map((item) => (
            <NavLink
              key={item.id}
              item={item}
              active={item.id === activeId}
              suffix="-mobile"
            />
          ))}
        </nav>

        <div className="mx-auto max-w-3xl">
          {!ready ? (
            <SettingsSkeleton />
          ) : (
            <div key={activeId} className="settings-enter">
              <p className="mb-3 text-xs text-muted">{trail}</p>
              <PageHeader
                icon={page.icon}
                title={page.title}
                description={page.description}
              />
              {children}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
