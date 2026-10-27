'use client';

import { useRouter } from 'next/navigation';
import { routes } from '../../lib/routes';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { IssueView } from './issue-view';

export function CardPage({
  workspaceId,
  cardId,
}: {
  workspaceId: string;
  cardId: string;
}) {
  const router = useRouter();
  const ctx = useWorkspaceCtx();
  return (
    <IssueView
      cardId={cardId}
      workspaceId={workspaceId}
      variant="page"
      onClose={() =>
        router.push(routes.workspace(ctx.workspace?.key_prefix ?? ''))
      }
    />
  );
}
