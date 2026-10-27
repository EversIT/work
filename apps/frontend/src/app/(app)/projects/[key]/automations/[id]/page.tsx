import { AutomationRoute } from '../../../../../../components/shell/route-pages';

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ key: string; id: string }>;
  searchParams: Promise<{ list?: string }>;
}) {
  const { id } = await params;
  const { list } = await searchParams;
  return <AutomationRoute id={id} listId={list} />;
}
