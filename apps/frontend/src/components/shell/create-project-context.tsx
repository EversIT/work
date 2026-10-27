'use client';

import { createContext, useContext, useMemo, useState } from 'react';
import { CreateProjectDialog } from './create-project-dialog';

type Ctx = { open: () => void };
const CreateProjectCtx = createContext<Ctx>({ open: () => undefined });

export const useCreateProject = () => useContext(CreateProjectCtx);

/** Lets any part of the shell open the "Create project" dialog. */
export function CreateProjectProvider({
  workspaceId,
  children,
}: {
  workspaceId: string;
  children: React.ReactNode;
}) {
  const [isOpen, setOpen] = useState(false);
  const value = useMemo(() => ({ open: () => setOpen(true) }), []);
  return (
    <CreateProjectCtx.Provider value={value}>
      {children}
      {isOpen && (
        <CreateProjectDialog
          workspaceId={workspaceId}
          onClose={() => setOpen(false)}
        />
      )}
    </CreateProjectCtx.Provider>
  );
}
