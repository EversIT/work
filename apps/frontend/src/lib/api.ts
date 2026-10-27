import { ApiError } from '../api/fetcher';

/** Generated hooks return `{status, data}` unions; the fetcher throws on errors, so this narrows to the success body. */
export function body<T extends { status: number; data: unknown }>(
  res: T | undefined,
): Extract<T, { status: 200 }>['data'] | undefined {
  return res && res.status === 200
    ? ((res as Extract<T, { status: 200 }>).data as Extract<
        T,
        { status: 200 }
      >['data'])
    : undefined;
}

export type ErrorKind =
  | 'unauthorized'
  | 'forbidden'
  | 'notFound'
  | 'conflict'
  | 'tooMany'
  | 'invalid'
  | 'generic';

export function errorKind(e: unknown): ErrorKind {
  if (!(e instanceof ApiError)) return 'generic';
  switch (e.status) {
    case 400:
      return 'invalid';
    case 401:
      return 'unauthorized';
    case 403:
      return 'forbidden';
    case 404:
      return 'notFound';
    case 409:
      return 'conflict';
    case 429:
      return 'tooMany';
    default:
      return 'generic';
  }
}

/** The server's validation message (safe, never contains internals) for 400s. */
export function serverMessage(e: unknown): string | undefined {
  if (
    e instanceof ApiError &&
    e.status === 400 &&
    e.body &&
    typeof e.body === 'object'
  ) {
    const m = (e.body as { error?: unknown }).error;
    return typeof m === 'string' ? m : undefined;
  }
  return undefined;
}
