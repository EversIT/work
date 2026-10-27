'use client';

import {
  ArrowLeft,
  Calendar,
  GanttChart,
  KanbanSquare,
  List as ListIcon,
  ListChecks,
  Settings,
  Table2,
} from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useParams, usePathname, useSearchParams } from 'next/navigation';
import { useGetListBySlug } from '../../api/generated/lists/lists';
import { body } from '../../lib/api';
import { routes } from '../../lib/routes';
import { useWorkspaceCtx } from './workspace-context';

const VIEW_ICON = {
  board: KanbanSquare,
  list: ListIcon,
  calendar: Calendar,
  gantt: GanttChart,
  table: Table2,
} as const;

function NavItem({
  href,
  active,
  icon: Icon,
  children,
  testId,
}: {
  href: string;
  active: boolean;
  icon: React.ComponentType<{ size?: number }>;
  children: React.ReactNode;
  testId?: string;
}) {
  return (
    <Link
      href={href}
      data-testid={testId}
      aria-current={active ? 'page' : undefined}
      className={`flex h-8 items-center gap-2.5 rounded-sm px-2 text-sm ${active ? 'bg-accent-soft font-medium text-accent' : 'text-muted hover:bg-default'}`}
    >
      <Icon size={16} />
      <span className="truncate">{children}</span>
    </Link>
  );
}

/**
 * The project sidebar, as in Jira Cloud: it only exists inside a project and holds that project's
 * views. Everything above the project (switching workspace, all projects, creating one) lives in
 * the top bar, so no separate "space" step is needed to get started.
 */
export function SideNav({ onNavigate }: { onNavigate?: () => void }) {
  const t = useTranslations('nav');
  const ctx = useWorkspaceCtx();
  const params = useParams<{ slug?: string }>();
  const pathname = usePathname();
  const search = useSearchParams();
  const slug = params.slug;
  const brief = ctx.lists.find((l) => l.slug === slug);
  const list = body(
    useGetListBySlug(ctx.workspaceId, slug ?? '', {
      query: { enabled: !!slug },
    }).data,
  );
  const base = routes.workspace(ctx.workspace?.key_prefix ?? '');
  const activeView = search.get('view') ?? list?.views[0]?.id;
  if (!list || !brief)
    return (
      <div className="h-full bg-surface-secondary" data-testid="side-nav" />
    );

  return (
    <div
      className="flex h-full flex-col gap-3 bg-surface-secondary p-3"
      data-testid="side-nav"
    >
      <Link
        href={base}
        onClick={onNavigate}
        className="flex h-8 items-center gap-2 rounded-sm px-2 text-xs font-medium text-muted hover:bg-default"
        data-testid="back-to-projects"
      >
        <ArrowLeft size={14} />
        {t('allProjects')}
      </Link>
      <div className="flex items-center gap-2.5 px-1">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-sm bg-gradient-to-br from-accent-hover to-accent text-sm font-bold text-white">
          {brief.name.slice(0, 2).toUpperCase()}
        </span>
        <div className="min-w-0">
          <p
            className="truncate text-sm font-semibold text-foreground"
            data-testid="side-list-name"
          >
            {brief.name}
          </p>
          <p className="text-xs text-muted">
            {brief.methodology === 'scrum'
              ? t('scrumProject')
              : t('kanbanProject')}
          </p>
        </div>
      </div>
      <nav aria-label={t('project')} className="flex flex-col gap-0.5">
        <p className="jira-eyebrow px-2 pb-1 pt-2">{t('planning')}</p>
        {list.views
          .filter((v) => v.type === 'gantt')
          .map((v) => (
            <NavItem
              key={v.id}
              href={`${base}/lists/${slug}?view=${v.id}`}
              active={
                pathname === `${base}/lists/${slug}` && activeView === v.id
              }
              icon={VIEW_ICON.gantt}
              testId={`nav-view-${v.name}`}
            >
              {v.name}
            </NavItem>
          ))}
        {brief.methodology === 'scrum' && (
          <NavItem
            href={`${base}/lists/${slug}/backlog`}
            active={pathname.endsWith('/backlog')}
            icon={ListChecks}
            testId="nav-backlog"
          >
            {t('backlog')}
          </NavItem>
        )}
        {list.views
          .filter((v) => v.type !== 'gantt')
          .map((v) => {
            const Icon = VIEW_ICON[v.type as keyof typeof VIEW_ICON] ?? Table2;
            return (
              <NavItem
                key={v.id}
                href={`${base}/lists/${slug}?view=${v.id}`}
                active={
                  pathname === `${base}/lists/${slug}` && activeView === v.id
                }
                icon={Icon}
                testId={`nav-view-${v.name}`}
              >
                {v.name}
              </NavItem>
            );
          })}
      </nav>
      <div className="my-1 border-t border-border" />
      <NavItem
        href={`${base}/lists/${slug}/settings`}
        active={pathname.includes('/settings')}
        icon={Settings}
      >
        {t('projectSettings')}
      </NavItem>
    </div>
  );
}
