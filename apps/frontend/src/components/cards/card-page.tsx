'use client';

import { useRouter } from 'next/navigation';
import { IssueView } from './issue-view';

export function CardPage({ workspaceId, cardId }: { workspaceId: string; cardId: string }) {
  const router = useRouter();
  return <IssueView cardId={cardId} workspaceId={workspaceId} variant="page" onClose={() => router.push(`/workspaces/${workspaceId}`)} />;
}
