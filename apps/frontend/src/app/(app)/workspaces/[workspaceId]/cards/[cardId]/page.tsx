import { CardPage } from '../../../../../../components/cards/card-page';

export default async function Page({ params }: { params: Promise<{ workspaceId: string; cardId: string }> }) {
  const { workspaceId, cardId } = await params;
  return <CardPage workspaceId={workspaceId} cardId={cardId} />;
}
