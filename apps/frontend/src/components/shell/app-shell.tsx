'use client';

import { Drawer, useOverlayState } from '@heroui/react';
import { useParams, usePathname } from 'next/navigation';
import { useEffect, useState } from 'react';
import { CreateIssueProvider } from './create-issue-context';
import { SideNav } from './side-nav';
import { TopNav } from './top-nav';
import { WorkspaceProvider } from './workspace-context';

/** Jira layout: a top bar, a project sidebar and the page. The sidebar needs a workspace. */
export function AppShell({ children }: { children: React.ReactNode }) {
  const params = useParams<{ workspaceId?: string }>();
  const pathname = usePathname();
  const drawer = useOverlayState();
  const [remembered, setRemembered] = useState<string | undefined>();
  useEffect(() => {
    try {
      setRemembered(localStorage.getItem('work.lastWorkspace') ?? undefined);
    } catch {
      /* storage may be unavailable */
    }
  }, [pathname]); // pages outside /workspaces/... keep the workspace you were last in
  const workspaceId = params.workspaceId ?? remembered;

  const chrome = (inner: React.ReactNode) => (
    <div className="flex min-h-screen flex-col bg-white text-[#172B4D]">
      <TopNav onMenu={drawer.open} />
      {inner}
    </div>
  );

  if (!workspaceId) {
    return chrome(<main className="min-w-0 flex-1">{children}</main>);
  }
  return (
    <WorkspaceProvider key={workspaceId} workspaceId={workspaceId}>
      <CreateIssueProvider workspaceId={workspaceId}>
        {chrome(
          <div className="flex min-h-0 flex-1">
            <aside className="hidden w-64 shrink-0 border-r border-[#DCDFE4] md:block">
              <SideNav />
            </aside>
            <Drawer state={drawer}>
              <Drawer.Backdrop>
                <Drawer.Content placement="left">
                  <Drawer.Dialog className="w-72">
                    <Drawer.Body className="p-0">
                      <SideNav onNavigate={drawer.close} />
                    </Drawer.Body>
                  </Drawer.Dialog>
                </Drawer.Content>
              </Drawer.Backdrop>
            </Drawer>
            <main className="min-w-0 flex-1">{children}</main>
          </div>,
        )}
      </CreateIssueProvider>
    </WorkspaceProvider>
  );
}
