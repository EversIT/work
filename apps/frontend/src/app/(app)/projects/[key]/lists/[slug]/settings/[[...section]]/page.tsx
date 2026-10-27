import { notFound } from 'next/navigation';
import { ProjectSettingsRoute } from '../../../../../../../../components/shell/route-pages';

export default async function Page({
  params,
}: {
  params: Promise<{ key: string; slug: string; section?: string[] }>;
}) {
  const { slug, section } = await params;
  if (section && section.length > 1) notFound();
  return <ProjectSettingsRoute slug={slug} segment={section?.[0]} />;
}
