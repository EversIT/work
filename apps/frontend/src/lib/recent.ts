const KEY = 'work.recentProjects';
const MAX = 8;

type Store = Record<string, string[]>;

function load(): Store {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : {};
    return parsed && typeof parsed === 'object' ? (parsed as Store) : {};
  } catch {
    return {};
  }
}

/** Slugs of the projects opened last in a workspace, newest first. Stored in this browser only. */
export function readRecentProjects(workspaceId: string): string[] {
  const list = load()[workspaceId];
  return Array.isArray(list) ? list.filter((s) => typeof s === 'string') : [];
}

export function rememberProject(workspaceId: string, slug: string) {
  try {
    const store = load();
    store[workspaceId] = [
      slug,
      ...readRecentProjects(workspaceId).filter((s) => s !== slug),
    ].slice(0, MAX);
    localStorage.setItem(KEY, JSON.stringify(store));
  } catch {
    // storage may be unavailable; the menu then simply lists projects in their own order
  }
}
