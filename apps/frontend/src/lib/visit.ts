/**
 * "Since your last visit": the start of the visit before this one. A visit is a stretch of use with no
 * longer pause than `GAP_MS`; it starts when someone opens the page after such a pause. The previous
 * visit's start is kept for the whole current visit, so reloading the page does not empty the summary.
 */
export const GAP_MS = 30 * 60 * 1000;
const DEFAULT_DAYS = 7;
const MAX_DAYS = 30;

type Store = Pick<Storage, 'getItem' | 'setItem'>;

export function sinceLastVisit(
  store: Store | null,
  workspace: string,
  now = new Date(),
): Date {
  const fallback = new Date(now.getTime() - DEFAULT_DAYS * 86_400_000);
  const floor = new Date(now.getTime() - MAX_DAYS * 86_400_000);
  if (!store) return fallback;
  const key = (k: string) => `work.${k}.${workspace}`;
  const read = (k: string) => {
    try {
      const t = Date.parse(store.getItem(key(k)) ?? '');
      return Number.isFinite(t) ? t : null;
    } catch {
      return null;
    }
  };
  const write = (k: string, v: Date) => {
    try {
      store.setItem(key(k), v.toISOString());
    } catch {
      /* private mode: the summary simply covers a week */
    }
  };
  const lastSeen = read('lastSeen');
  const visitStart = read('visitStart');
  const since = read('since');
  const newVisit = lastSeen === null || now.getTime() - lastSeen > GAP_MS;
  write('lastSeen', now);
  if (newVisit) {
    // The visit that just ended is the one to catch up on.
    const previous = visitStart !== null ? new Date(visitStart) : fallback;
    write('visitStart', now);
    write('since', previous);
    return previous < floor ? floor : previous;
  }
  const kept = since !== null ? new Date(since) : fallback;
  return kept < floor ? floor : kept;
}
