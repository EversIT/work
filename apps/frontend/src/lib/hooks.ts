'use client';

import { useQueries, useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { listWorkspaces } from '../api/generated/workspaces/workspaces';
import { useListProjects } from '../api/generated/projects/projects';
import { useMe } from '../api/generated/auth/auth';
import { useListCardTypes } from '../api/generated/card-types/card-types';
import { useListProjectMembers } from '../api/generated/projects/projects';
import { getWorkspace } from '../api/generated/workspaces/workspaces';
import { body } from './api';

export function useCurrentUser() {
  return body(useMe().data);
}

export function useProjects() {
  return body(useListProjects().data) ?? [];
}

/** All workspaces the user can reach, each with the project it belongs to. */
export function useAllWorkspaces() {
  const projectsQuery = useListProjects();
  const projects = useMemo(() => body(projectsQuery.data) ?? [], [projectsQuery.data]);
  const results = useQueries({
    queries: projects.map((p) => ({
      queryKey: ['workspaces', p.id],
      queryFn: () => listWorkspaces(p.id),
    })),
  });
  // Not loading until the project list AND every workspace list has arrived, otherwise UIs
  // that depend on "no workspaces yet" flash and discard user input.
  const loading = projectsQuery.isLoading || results.some((r) => r.isLoading);
  const workspaces = useMemo(
    () =>
      results.flatMap((r, i) => {
        const list = r.data && r.data.status === 200 ? r.data.data : [];
        return list.map((w) => ({ ...w, project: projects[i] }));
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [JSON.stringify(results.map((r) => r.dataUpdatedAt)), projects],
  );
  return { workspaces, projects, loading };
}

export function useWorkspace(workspaceId: string) {
  return body(
    useQuery({ queryKey: ['workspace', workspaceId], queryFn: () => getWorkspace(workspaceId) }).data,
  );
}

export function useCardTypes(workspaceId: string) {
  return body(useListCardTypes(workspaceId).data) ?? [];
}

export function useMembers(projectId: string | undefined) {
  return (
    body(useListProjectMembers(projectId ?? '', { query: { enabled: !!projectId, refetchInterval: 30_000 } }).data) ?? []
  );
}
