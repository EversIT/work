'use client';

import { Drawer, Spinner, useOverlayState } from '@heroui/react';
import { useParams, usePathname } from 'next/navigation';
import { useTranslations } from 'next-intl';
import { useMemo } from 'react';
import { useWorkspaceByKey } from '../../lib/hooks';
import { parseIssueKey } from '../../lib/routes';
import { isDemo } from '../../demo';
import { CreateIssueProvider } from './create-issue-context';
import { CreateProjectProvider } from './create-project-context';
import { MobileNav } from './mobile-nav';
import { SideNav } from './side-nav';
import { TopNav } from './top-nav';
import { WorkspaceProvider } from './workspace-context';

/**
 * Jira Cloud layout: a top bar everywhere, and a project sidebar only inside a project. Pages
 * outside a project (all projects, issues, settings) use the full width.
 */
export function AppShell({ children }: { children: React.ReactNode }) {
  const params = useParams<{
    key?: string;
    issueKey?: string;
    slug?: string;
  }>();
  const pathname = usePathname();
  const drawer = useOverlayState();
  const t = useTranslations('demo');
  const t2 = useTranslations('home');
  // Addresses name the workspace by its key (`/projects/ACME`, `/browse/ACME-1`); pages outside a
  // workspace keep the one you were last in. Re-read on every navigation.
  const remembered = useMemo(() => {
    try {
      return localStorage.getItem('work.lastWorkspace') ?? undefined;
    } catch {
      return undefined; // storage may be unavailable
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pathname]);
  const keyParam =
    params.key ??
    (params.issueKey ? parseIssueKey(params.issueKey)?.key : undefined);
  const byKey = useWorkspaceByKey(keyParam);
  const workspaceId = keyParam ? byKey.workspace?.id : remembered;
  // Project settings bring their own sidebar.
  const inProject =
    /^\/projects\/[^/]+\/lists\//.test(pathname) &&
    !pathname.includes('/settings');

  const chrome = (inner: React.ReactNode, hasMenu = false) => (
    <div className="flex min-h-screen flex-col bg-surface text-foreground">
      {isDemo() && (
        <p
          className="bg-accent px-3 py-1 text-center text-xs text-accent-foreground"
          data-testid="demo-banner"
        >
          {t('banner')}
        </p>
      )}
      <TopNav onMenu={drawer.open} showMenu={hasMenu} />
      {inner}
    </div>
  );

  if (keyParam && !byKey.workspace) {
    return chrome(
      <main className="min-w-0 flex-1">
        {byKey.failed ? (
          <p
            className="p-12 text-center text-muted"
            data-testid="workspace-not-found"
          >
            {t2('notFound')}
          </p>
        ) : (
          <div className="flex justify-center p-12">
            <Spinner aria-label={t2('loading')} />
          </div>
        )}
      </main>,
    );
  }
  if (!workspaceId) {
    return chrome(<main className="min-w-0 flex-1">{children}</main>);
  }
  return (
    <WorkspaceProvider
      key={workspaceId}
      workspaceId={workspaceId}
      initial={byKey.workspace}
    >
      <CreateIssueProvider workspaceId={workspaceId}>
        <CreateProjectProvider workspaceId={workspaceId}>
          {chrome(
            <div className="flex min-h-0 flex-1">
              {inProject && (
                <aside className="hidden w-64 shrink-0 border-r border-border md:block">
                  <SideNav />
                </aside>
              )}
              <Drawer state={drawer}>
                <Drawer.Backdrop>
                  <Drawer.Content placement="left">
                    <Drawer.Dialog className="w-[85vw] max-w-80">
                      <Drawer.Body className="p-0">
                        <MobileNav
                          inProject={inProject}
                          onNavigate={drawer.close}
                        />
                      </Drawer.Body>
                    </Drawer.Dialog>
                  </Drawer.Content>
                </Drawer.Backdrop>
              </Drawer>
              <main className="min-w-0 flex-1">{children}</main>
            </div>,
            true,
          )}
        </CreateProjectProvider>
      </CreateIssueProvider>
    </WorkspaceProvider>
  );
}
