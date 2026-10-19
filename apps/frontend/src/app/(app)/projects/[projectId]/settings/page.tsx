import { ProjectSettings } from '../../../../../components/settings/project-settings';

export default async function Page({ params }: { params: Promise<{ projectId: string }> }) {
  const { projectId } = await params;
  return <ProjectSettings projectId={projectId} />;
}
