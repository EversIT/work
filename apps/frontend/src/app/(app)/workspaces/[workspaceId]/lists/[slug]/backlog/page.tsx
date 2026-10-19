import { Suspense } from 'react';
import { BacklogPage } from '../../../../../../../components/views/backlog-page';

export default async function Page({ params }: { params: Promise<{ workspaceId: string; slug: string }> }) {
  const { workspaceId, slug } = await params;
  return (
    <Suspense>
      <BacklogPage workspaceId={workspaceId} slug={slug} />
    </Suspense>
  );
}
