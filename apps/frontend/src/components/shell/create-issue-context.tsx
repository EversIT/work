'use client';

import { createContext, useCallback, useContext, useState } from 'react';
import { CreateIssueDialog } from '../cards/create-issue-dialog';

type Ctx = { open: (opts?: { listId?: string; stageId?: string; sprintId?: string | null; parentId?: string }) => void };
const C = createContext<Ctx>({ open: () => undefined });

export const useCreateIssue = () => useContext(C);

/** One create-issue dialog for the whole app: the top bar, boards and backlogs all open the same one. */
export function CreateIssueProvider({ workspaceId, children }: { workspaceId?: string; children: React.ReactNode }) {
  const [state, setState] = useState<{ listId?: string; stageId?: string; sprintId?: string | null; parentId?: string } | null>(null);
  const open = useCallback((opts?: { listId?: string; stageId?: string; sprintId?: string | null; parentId?: string }) => setState(opts ?? {}), []);
  return (
    <C.Provider value={{ open }}>
      {children}
      {workspaceId && state && <CreateIssueDialog {...state} onClose={() => setState(null)} />}
    </C.Provider>
  );
}
