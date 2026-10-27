'use client';

import { usePathname, useRouter, useSearchParams } from 'next/navigation';
import { createContext, useContext, useEffect, useState } from 'react';
import { hasAccessToken, refreshAccessToken } from '../api/fetcher';

type Status = 'loading' | 'authed' | 'anon';
const Ctx = createContext<Status>('loading');

export const useAuthStatus = () => useContext(Ctx);

/**
 * The access token only lives in memory, so after a reload the session is restored from the
 * HttpOnly refresh cookie. Children render only once a valid session exists.
 */
export function AuthGate({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const path = usePathname();
  const search = useSearchParams().toString();
  const [status, setStatus] = useState<Status>(
    hasAccessToken() ? 'authed' : 'loading',
  );

  useEffect(() => {
    if (status !== 'loading') return;
    let cancelled = false;
    refreshAccessToken().then((ok) => {
      if (!cancelled) setStatus(ok ? 'authed' : 'anon');
    });
    return () => {
      cancelled = true;
    };
  }, [status]);

  useEffect(() => {
    if (status === 'anon') {
      const full = path ? `${path}${search ? `?${search}` : ''}` : '/';
      const next = full !== '/' ? `?next=${encodeURIComponent(full)}` : '';
      router.replace(`/login${next}`);
    }
  }, [status, router, path, search]);

  if (status !== 'authed') return null;
  return <Ctx.Provider value={status}>{children}</Ctx.Provider>;
}
