/** Only same-site relative paths are accepted as post-login targets (no open redirects). */
export function safeNext(value: string | null | undefined): string {
  if (
    !value ||
    !value.startsWith('/') ||
    value.startsWith('//') ||
    value.startsWith('/\\') ||
    [...value].some((c) => c.charCodeAt(0) < 32)
  )
    return '/';
  return value;
}

const PENDING_NEXT = 'work.pendingNext';
const PENDING_NEXT_TTL_MS = 24 * 60 * 60 * 1000; // as long as the confirmation link lives

/**
 * Sign-up with e-mail confirmation leaves the page, so the target (e.g. an invitation) is kept in
 * the browser until the link is opened. It is only a same-site path, never a credential, and
 * `localStorage` (not `sessionStorage`) because the link usually opens in a new tab.
 */
export function rememberNext(next: string) {
  try {
    if (next !== '/')
      localStorage.setItem(
        PENDING_NEXT,
        JSON.stringify({ next, at: Date.now() }),
      );
  } catch {
    // storage may be unavailable (private mode); the user then lands on the home page
  }
}

export function takePendingNext(): string {
  try {
    const raw = localStorage.getItem(PENDING_NEXT);
    localStorage.removeItem(PENDING_NEXT);
    const saved = raw
      ? (JSON.parse(raw) as { next?: string; at?: number })
      : null;
    if (
      saved &&
      typeof saved.at === 'number' &&
      Date.now() - saved.at < PENDING_NEXT_TTL_MS
    )
      return safeNext(saved.next);
  } catch {
    // fall through
  }
  return '/';
}
