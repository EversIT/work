'use client';

import { Dropdown, buttonVariants } from '@heroui/react';
import { Calendar, ChevronDown, Folder, GanttChart, KanbanSquare, List as ListIcon, ListChecks, Lock, Plus, Settings, Table2 } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useParams, usePathname, useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { useGetListBySlug } from '../../api/generated/lists/lists';
import { body } from '../../lib/api';
import { useAllWorkspaces } from '../../lib/hooks';
import { CreateListDialog } from './create-list-dialog';
import { CreateSpaceDialog } from './create-space-dialog';
import { useWorkspaceCtx } from './workspace-context';

const VIEW_ICON = { board: KanbanSquare, list: ListIcon, calendar: Calendar, gantt: GanttChart, table: Table2 } as const;

function NavItem({ href, active, icon: Icon, children, testId }: { href: string; active: boolean; icon: React.ComponentType<{ size?: number }>; children: React.ReactNode; testId?: string }) {
  return (
    <Link
      href={href}
      data-testid={testId}
      aria-current={active ? 'page' : undefined}
      className={`flex h-8 items-center gap-2.5 rounded-[3px] px-2 text-sm ${active ? 'bg-[#E9F2FF] font-medium text-[#0C66E4]' : 'text-[#44546F] hover:bg-[#091E420F]'}`}
    >
      <Icon size={16} />
      <span className="truncate">{children}</span>
    </Link>
  );
}

export function SideNav({ onNavigate }: { onNavigate?: () => void }) {
  const t = useTranslations('nav');
  const ctx = useWorkspaceCtx();
  const router = useRouter();
  const params = useParams<{ slug?: string }>();
  const pathname = usePathname();
  const search = useSearchParams();
  const { workspaces } = useAllWorkspaces();
  const slug = params.slug;
  const brief = ctx.lists.find((l) => l.slug === slug);
  const list = body(useGetListBySlug(ctx.workspaceId, slug ?? '', { query: { enabled: !!slug } }).data);
  const [spaceDialog, setSpaceDialog] = useState(false);
  const [listDialog, setListDialog] = useState<string | null>(null);
  const base = `/workspaces/${ctx.workspaceId}`;
  const activeView = search.get('view') ?? list?.views[0]?.id;

  const switcher = (
    <Dropdown>
      <Dropdown.Trigger className={buttonVariants({ variant: 'ghost', className: 'flex h-auto w-full items-center justify-between px-2 py-1.5 text-left' })} data-testid="workspace-switcher">
        <span className="min-w-0">
          <span className="block truncate text-sm font-semibold text-[#172B4D]">{ctx.workspace?.name ?? t('chooseWorkspace')}</span>
          <span className="block truncate text-xs text-muted">{ctx.workspace?.key_prefix}</span>
        </span>
        <ChevronDown size={14} />
      </Dropdown.Trigger>
      <Dropdown.Popover>
        <Dropdown.Menu
          aria-label={t('chooseWorkspace')}
          onAction={(key) => {
            router.push(`/workspaces/${String(key)}`);
            onNavigate?.();
          }}
        >
          {workspaces.map((w) => (
            <Dropdown.Item key={w.id} id={w.id} textValue={w.name}>
              <div className="flex flex-col">
                <span>{w.name}</span>
                <span className="text-xs text-muted">{w.project.name}</span>
              </div>
            </Dropdown.Item>
          ))}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );

  return (
    <div className="flex h-full flex-col gap-3 bg-[#F7F8F9] p-3" data-testid="side-nav">
      {list && brief ? (
        <>
          <div className="flex items-center gap-2.5 px-1 pt-1">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-[4px] bg-gradient-to-br from-[#1D7AFC] to-[#0C66E4] text-sm font-bold text-white">{brief.name.slice(0, 2).toUpperCase()}</span>
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold text-[#172B4D]" data-testid="side-list-name">{brief.name}</p>
              <p className="text-xs text-muted">{brief.methodology === 'scrum' ? t('scrumProject') : t('kanbanProject')}</p>
            </div>
          </div>
          <nav aria-label={t('project')} className="flex flex-col gap-0.5">
            <p className="jira-eyebrow px-2 pb-1 pt-2">{t('planning')}</p>
            {list.views.filter((v) => v.type === 'gantt').map((v) => (
              <NavItem key={v.id} href={`${base}/lists/${slug}?view=${v.id}`} active={pathname === `${base}/lists/${slug}` && activeView === v.id} icon={VIEW_ICON.gantt} testId={`nav-view-${v.name}`}>{v.name}</NavItem>
            ))}
            {brief.methodology === 'scrum' && (
              <NavItem href={`${base}/lists/${slug}/backlog`} active={pathname.endsWith('/backlog')} icon={ListChecks} testId="nav-backlog">{t('backlog')}</NavItem>
            )}
            {list.views.filter((v) => v.type !== 'gantt').map((v) => {
              const Icon = VIEW_ICON[v.type as keyof typeof VIEW_ICON] ?? Table2;
              return (
                <NavItem key={v.id} href={`${base}/lists/${slug}?view=${v.id}`} active={pathname === `${base}/lists/${slug}` && activeView === v.id} icon={Icon} testId={`nav-view-${v.name}`}>
                  {v.name}
                </NavItem>
              );
            })}
          </nav>
          <div className="my-1 border-t border-[#DCDFE4]" />
          <NavItem href={`${base}/settings`} active={pathname.endsWith('/settings')} icon={Settings}>{t('projectSettings')}</NavItem>
          <div className="mt-auto">
            <p className="jira-eyebrow px-2 pb-1">{t('workspace')}</p>
            {switcher}
            <Link href={base} onClick={onNavigate} className="flex h-8 items-center rounded-[3px] px-2 text-sm text-[#44546F] hover:bg-[#091E420F]">{t('allProjects')}</Link>
          </div>
        </>
      ) : (
        <>
          {switcher}
          <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto" aria-label={t('spaces')}>
            <NavItem href={base} active={pathname === base} icon={ListChecks} testId="nav-your-work">{t('yourWork')}</NavItem>
            <div className="flex items-center justify-between px-2 pb-1 pt-3">
              <span className="jira-eyebrow">{t('projects')}</span>
              <button className="rounded p-1 text-[#44546F] hover:bg-[#091E420F]" aria-label={t('newSpace')} onClick={() => setSpaceDialog(true)} data-testid="new-space">
                <Plus size={14} />
              </button>
            </div>
            {ctx.spaces.map((s) => (
              <div key={s.id} className="mb-1">
                <div className="flex items-center justify-between rounded-[3px] px-2 py-1 hover:bg-[#091E420F]">
                  <span className="flex min-w-0 items-center gap-2 text-sm font-medium text-[#172B4D]">
                    <Folder size={14} />
                    <span className="truncate">{s.name}</span>
                    {s.access_type === 'private' && <Lock size={12} className="text-muted" aria-label={t('private')} />}
                  </span>
                  <button className="rounded p-1 text-[#44546F] hover:bg-[#091E4214]" aria-label={t('newList')} onClick={() => setListDialog(s.id)} data-testid={`new-list-${s.name}`}>
                    <Plus size={14} />
                  </button>
                </div>
                {s.lists.map((l) => (
                  <Link key={l.id} href={`${base}/lists/${l.slug}`} onClick={onNavigate} className="ml-5 flex h-8 items-center gap-2 rounded-[3px] px-2 text-sm text-[#44546F] hover:bg-[#091E420F]">
                    <span className="flex size-4 items-center justify-center rounded-[3px] bg-[#0C66E4] text-[8px] font-bold text-white">{l.name.slice(0, 1).toUpperCase()}</span>
                    <span className="truncate">{l.name}</span>
                    {l.access_type === 'private' && <Lock size={12} className="text-muted" />}
                  </Link>
                ))}
              </div>
            ))}
          </nav>
          <NavItem href={`${base}/settings`} active={pathname.endsWith('/settings')} icon={Settings}>{t('workspaceSettings')}</NavItem>
        </>
      )}
      <CreateSpaceDialog workspaceId={ctx.workspaceId} isOpen={spaceDialog} onClose={() => setSpaceDialog(false)} />
      {listDialog && <CreateListDialog workspaceId={ctx.workspaceId} spaceId={listDialog} isOpen onClose={() => setListDialog(null)} />}
    </div>
  );
}
