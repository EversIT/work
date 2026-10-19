import { Suspense } from 'react';
import { ListPage } from '../../../../../../components/views/list-page';

export default async function Page({ params }: { params: Promise<{ workspaceId: string; slug: string }> }) {
  const { workspaceId, slug } = await params;
  return (
    <Suspense>
      <ListPage workspaceId={workspaceId} slug={slug} />
    </Suspense>
  );
}
