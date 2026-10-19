import { Suspense } from 'react';
import { AppShell } from '../../components/shell/app-shell';
import { AuthGate } from '../../lib/auth';

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <Suspense>
      <AuthGate>
        <AppShell>{children}</AppShell>
      </AuthGate>
    </Suspense>
  );
}
