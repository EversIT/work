import { notFound } from 'next/navigation';
import { AccountSettingsShell } from '../../../../components/settings/account-settings-shell';

export default async function Page({
  params,
}: {
  params: Promise<{ section?: string[] }>;
}) {
  const { section } = await params;
  if (section && section.length > 1) notFound();
  return <AccountSettingsShell segment={section?.[0]} />;
}
