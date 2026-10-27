/**
 * Fetch mutator used by the orval-generated client.
 *
 * - Requests go to `/api/v1/...` on the frontend origin; the Next proxy forwards them to the
 *   backend, so the browser never talks to a second origin or port.
 * - The access token lives in module memory only (never localStorage), so a stored-XSS bug
 *   cannot exfiltrate a long-lived credential. The refresh token is an HttpOnly cookie set
 *   by the backend and is invisible to scripts.
 * - On a 401 the request is retried once after a single-flight token refresh.
 */

import { DEMO } from '../demo';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: unknown,
  ) {
    super(`API request failed with status ${status}`);
    this.name = 'ApiError';
  }
}

let accessToken: string | null = null;
let refreshing: Promise<boolean> | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export function currentAccessToken() {
  return accessToken;
}

export function hasAccessToken() {
  return accessToken !== null;
}

const AUTH_PATHS = [
  '/api/v1/auth/login',
  '/api/v1/auth/register',
  '/api/v1/auth/verify-email',
  '/api/v1/auth/mfa/verify',
  '/api/v1/auth/passkey/login',
  '/api/v1/auth/refresh',
];

export async function refreshAccessToken(): Promise<boolean> {
  if (DEMO) {
    accessToken = 'demo';
    return true;
  }
  refreshing ??= (async () => {
    try {
      const res = await fetch('/api/v1/auth/refresh', {
        method: 'POST',
        credentials: 'same-origin',
      });
      if (!res.ok) {
        accessToken = null;
        return false;
      }
      const body = (await res.json()) as { access_token: string };
      accessToken = body.access_token;
      return true;
    } catch {
      return false;
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

async function send(url: string, options: RequestInit): Promise<Response> {
  const headers = new Headers(options.headers);
  if (accessToken) headers.set('Authorization', `Bearer ${accessToken}`);
  return fetch(url, { ...options, headers, credentials: 'same-origin' });
}

export async function apiFetch<T>(
  url: string,
  options: RequestInit = {},
): Promise<T> {
  const path = url.split('?')[0];

  // Demo mode: answered in the browser from made-up data, nothing leaves the page.
  if (DEMO) {
    // Loaded on demand, so a normal build never ships the made-up data.
    const { demoRequest } = await import('../demo/server');
    const r = demoRequest(url, options);
    // A copy, so later edits to the demo data are seen as changes by the query cache.
    const data = r.data === undefined ? undefined : structuredClone(r.data);
    if (r.status >= 400) throw new ApiError(r.status, data);
    if (AUTH_PATHS.includes(path)) accessToken = 'demo';
    if (path === '/api/v1/auth/logout') accessToken = null;
    return { data, status: r.status, headers: new Headers() } as T;
  }

  let res = await send(url, options);

  if (
    res.status === 401 &&
    !AUTH_PATHS.includes(path) &&
    (await refreshAccessToken())
  ) {
    res = await send(url, options);
  }

  const text = await res.text();
  const data = text ? safeJson(text) : undefined;
  if (!res.ok) throw new ApiError(res.status, data);

  // Sign-in responses carry the access token; keep it in memory. (A first step that asks for a
  // second factor or a confirmation has none.)
  if (
    AUTH_PATHS.includes(path) &&
    data &&
    typeof data === 'object' &&
    typeof (data as { access_token?: unknown }).access_token === 'string'
  ) {
    accessToken = (data as { access_token: string }).access_token;
  }
  if (path === '/api/v1/auth/logout') accessToken = null;

  return { data, status: res.status, headers: res.headers } as T;
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}
