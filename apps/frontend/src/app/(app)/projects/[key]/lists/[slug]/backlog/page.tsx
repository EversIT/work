import { Suspense } from 'react';
import { BacklogRoute } from '../../../../../../../components/shell/route-pages';

export default async function Page({
  params,
}: {
  params: Promise<{ key: string; slug: string }>;
}) {
  const { slug } = await params;
  return (
    <Suspense>
      <BacklogRoute slug={slug} />
    </Suspense>
  );
}
