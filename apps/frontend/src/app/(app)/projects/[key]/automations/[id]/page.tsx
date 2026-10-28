import { AutomationRoute } from '../../../../../../components/shell/route-pages';

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ key: string; id: string }>;
  searchParams: Promise<{ list?: string; describe?: string }>;
}) {
  const { id } = await params;
  const { list, describe } = await searchParams;
  return <AutomationRoute id={id} listId={list} describe={describe} />;
}
