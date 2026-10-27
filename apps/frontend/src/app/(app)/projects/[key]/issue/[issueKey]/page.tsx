import { IssueRoute } from '../../../../../../components/shell/route-pages';

export default async function Page({
  params,
}: {
  params: Promise<{ key: string; issueKey: string }>;
}) {
  const { issueKey } = await params;
  return <IssueRoute issueKey={issueKey.toUpperCase()} />;
}
