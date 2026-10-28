import { useSyncExternalStore } from 'react';

/** True while the media query matches. The server and the first render say false (desktop). */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (notify) => {
      const m = window.matchMedia(query);
      m.addEventListener('change', notify);
      return () => m.removeEventListener('change', notify);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}

/** Below Tailwind's `md` breakpoint: phones. */
export const useIsPhone = () => useMediaQuery('(max-width: 767px)');
