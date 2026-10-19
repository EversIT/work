'use client';

import { createContext, useContext, useMemo } from 'react';
import type { CardTypeOut, ListBrief, SpaceTree, WorkspaceOut } from '../../api/generated/model';
import { useGetWorkspaceTree } from '../../api/generated/spaces/spaces';
import { body } from '../../lib/api';
import { useCardTypes, useCurrentUser, useMembers, useWorkspace } from '../../lib/hooks';
import type { Member } from '../views/types';

type Ctx = {
  workspaceId: string;
  workspace?: WorkspaceOut;
  spaces: SpaceTree[];
  lists: (ListBrief & { spaceName: string })[];
  cardTypes: CardTypeOut[];
  members: Member[];
  me?: { id: string; first_name: string; last_name: string; email: string };
};

const WorkspaceCtx = createContext<Ctx | null>(null);

export function WorkspaceProvider({ workspaceId, children }: { workspaceId: string; children: React.ReactNode }) {
  const workspace = useWorkspace(workspaceId);
  const tree = body(useGetWorkspaceTree(workspaceId).data);
  const cardTypes = useCardTypes(workspaceId);
  const members = useMembers(workspace?.project_id) as Member[];
  const me = useCurrentUser();
  const value = useMemo<Ctx>(
    () => ({
      workspaceId,
      workspace,
      spaces: tree?.spaces ?? [],
      lists: (tree?.spaces ?? []).flatMap((s) => s.lists.map((l) => ({ ...l, spaceName: s.name }))),
      cardTypes,
      members,
      me,
    }),
    [workspaceId, workspace, tree, cardTypes, members, me],
  );
  return <WorkspaceCtx.Provider value={value}>{children}</WorkspaceCtx.Provider>;
}

export function useWorkspaceCtx(): Ctx {
  const c = useContext(WorkspaceCtx);
  if (!c) throw new Error('useWorkspaceCtx must be used inside a workspace');
  return c;
}

export function useOptionalWorkspaceCtx(): Ctx | null {
  return useContext(WorkspaceCtx);
}
