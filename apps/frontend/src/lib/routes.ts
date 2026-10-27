/**
 * Every address of the app, in one place. A workspace is identified by its key (`ACME`, the prefix
 * of its issue keys), never by an id, so a link can be read and typed: `/projects/ACME`,
 * `/browse/ACME-12`.
 */
/** The path the app is served under (`/demo`), or nothing. Needed for links built by hand. */
export const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

export const routes = {
  home: () => '/',
  /** A workspace: its projects and the issues assigned to you. */
  workspace: (key: string) => `/projects/${key}`,
  /** A project (a board or a backlog) inside a workspace. */
  project: (key: string, slug: string) => `/projects/${key}/lists/${slug}`,
  backlog: (key: string, slug: string) =>
    `/projects/${key}/lists/${slug}/backlog`,
  projectSettings: (key: string, slug: string, section?: string) =>
    `/projects/${key}/lists/${slug}/settings${section ? `/${section}` : ''}`,
  workspaceSettings: (key: string, section?: string) =>
    `/projects/${key}/settings${section ? `/${section}` : ''}`,
  /** An issue reached from its workspace. */
  issue: (key: string, issueKey: string) =>
    `/projects/${key}/issue/${issueKey}`,
  /** The short address of an issue; what people share. */
  browse: (issueKey: string) => `/browse/${issueKey}`,
  automation: (key: string, id: string, listId?: string) =>
    `/projects/${key}/automations/${id}${listId ? `?list=${listId}` : ''}`,
};

/** `ACME-12` -> `{ key: 'ACME', number: 12 }`, or null when it is not an issue key. */
export function parseIssueKey(
  value: string,
): { key: string; number: number } | null {
  const m = /^([A-Za-z][A-Za-z0-9]{1,9})-([1-9][0-9]{0,8})$/.exec(value);
  return m ? { key: m[1].toUpperCase(), number: Number(m[2]) } : null;
}
