import { WorkspaceHome } from '../../../../components/shell/workspace-home';

export default async function Page({ params }: { params: Promise<{ workspaceId: string }> }) {
  const { workspaceId } = await params;
  return <WorkspaceHome workspaceId={workspaceId} />;
}
