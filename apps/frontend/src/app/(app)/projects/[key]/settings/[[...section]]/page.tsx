import { notFound } from 'next/navigation';
import { WorkspaceSettingsRoute } from '../../../../../../components/shell/route-pages';

export default async function Page({
  params,
}: {
  params: Promise<{ key: string; section?: string[] }>;
}) {
  const { section } = await params;
  if (section && section.length > 1) notFound();
  return <WorkspaceSettingsRoute segment={section?.[0]} />;
}
