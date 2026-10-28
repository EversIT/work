'use client';

import { Button } from '@heroui/react';
import { Home, Plus, Settings } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { routes } from '../../lib/routes';
import { useCreateProject } from './create-project-context';
import { SideNav } from './side-nav';
import { useWorkspaceCtx } from './workspace-context';

function Row({
  href,
  icon: Icon,
  children,
  onNavigate,
}: {
  href: string;
  icon: React.ComponentType<{ size?: number }>;
  children: React.ReactNode;
  onNavigate: () => void;
}) {
  return (
    <Link
      href={href}
      onClick={onNavigate}
      className="flex h-11 items-center gap-3 rounded-md px-3 text-sm text-foreground hover:bg-default"
    >
      <Icon size={18} />
      <span className="truncate">{children}</span>
    </Link>
  );
}

/**
 * The menu behind the hamburger on phones: what the top bar shows on a wide screen (your work,
 * projects, settings) plus, inside a project, that project's own sidebar.
 */
export function MobileNav({
  inProject,
  onNavigate,
}: {
  inProject: boolean;
  onNavigate: () => void;
}) {
  const t = useTranslations('nav');
  const ctx = useWorkspaceCtx();
  const createProject = useCreateProject();
  const wsKey = ctx.workspace?.key_prefix ?? '';
  return (
    <div className="flex flex-col gap-4 overflow-y-auto p-3">
      <nav aria-label={t('main')} className="flex flex-col gap-0.5">
        <Row href={routes.workspace(wsKey)} icon={Home} onNavigate={onNavigate}>
          {t('yourWork')}
        </Row>
        <Row
          href={routes.workspaceSettings(wsKey)}
          icon={Settings}
          onNavigate={onNavigate}
        >
          {t('workspaceSettings')}
        </Row>
      </nav>
      <section aria-label={t('projects')}>
        <p className="jira-eyebrow px-3 pb-1">{t('projects')}</p>
        <ul className="flex flex-col gap-0.5">
          {ctx.lists.map((l) => (
            <li key={l.id}>
              <Link
                href={routes.project(wsKey, l.slug)}
                onClick={onNavigate}
                className="flex h-11 items-center gap-3 rounded-md px-3 text-sm hover:bg-default"
              >
                <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-gradient-to-br from-accent-hover to-accent text-[10px] font-bold text-accent-foreground">
                  {l.name.slice(0, 2).toUpperCase()}
                </span>
                <span className="flex min-w-0 flex-col leading-tight">
                  <span className="truncate">{l.name}</span>
                  <span className="truncate text-xs text-muted">
                    {l.spaceName}
                  </span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
        <Button
          size="sm"
          variant="secondary"
          className="mt-2 w-full"
          onPress={() => {
            onNavigate();
            createProject.open();
          }}
        >
          <Plus size={14} />
          {t('createProject')}
        </Button>
      </section>
      {inProject && (
        <div className="border-t border-separator pt-3">
          <SideNav onNavigate={onNavigate} />
        </div>
      )}
    </div>
  );
}
