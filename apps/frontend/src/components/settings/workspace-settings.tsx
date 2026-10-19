'use client';

import { Button, Card, Tabs, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useDeleteWorkspace, useUpdateWorkspace } from '../../api/generated/workspaces/workspaces';
import { errorKind, serverMessage } from '../../lib/api';
import { useWorkspace } from '../../lib/hooks';
import { TextInput } from '../ui/inputs';
import { AutomationSettings } from './automation-settings';
import { CardTypeSettings } from './card-type-settings';
import { SpaceListSettings } from './space-list-settings';

export function WorkspaceSettings({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('settings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const ws = useWorkspace(workspaceId);
  const rename = useUpdateWorkspace();
  const del = useDeleteWorkspace();
  const [name, setName] = useState<string | null>(null);
  const fail = (e: unknown) => toast.danger(serverMessage(e) ?? te(errorKind(e)));
  if (!ws) return null;
  return (
    <div className="mx-auto max-w-4xl p-4">
      <h1 className="mb-4 text-xl font-semibold">{t('workspaceSettings')}</h1>
      <Tabs>
        <Tabs.ListContainer>
          <Tabs.List aria-label={t('workspaceSettings')}>
            <Tabs.Tab id="general" data-testid="tab-general">{t('general')}<Tabs.Indicator /></Tabs.Tab>
            <Tabs.Tab id="types" data-testid="tab-types">{t('cardTypes')}<Tabs.Indicator /></Tabs.Tab>
            <Tabs.Tab id="spaces" data-testid="tab-spaces">{t('spacesAndLists')}<Tabs.Indicator /></Tabs.Tab>
            <Tabs.Tab id="automations" data-testid="tab-automations">{t('automations')}<Tabs.Indicator /></Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel id="general" className="pt-4">
          <Card>
            <Card.Content className="flex flex-col gap-3">
              <TextInput label={t('workspaceName')} value={name ?? ws.name} onChange={setName} maxLength={100} />
              <div className="flex flex-wrap gap-2">
                <Button isDisabled={!name || name === ws.name} onPress={async () => { try { await rename.mutateAsync({ id: workspaceId, data: { name: name ?? '' } }); await qc.invalidateQueries(); setName(null); toast.success(t('saved')); } catch (e) { fail(e); } }}>{t('save')}</Button>
                <Link href={`/projects/${ws.project_id}/settings`} className="inline-flex h-9 items-center rounded-lg px-3 text-sm underline">{t('projectMembers')}</Link>
                <Button variant="danger-soft" onPress={async () => { if (!window.confirm(t('confirmDeleteWorkspace'))) return; try { await del.mutateAsync({ id: workspaceId }); await qc.invalidateQueries(); router.replace('/'); } catch (e) { fail(e); } }}>{t('deleteWorkspace')}</Button>
              </div>
            </Card.Content>
          </Card>
        </Tabs.Panel>
        <Tabs.Panel id="types" className="pt-4"><CardTypeSettings workspaceId={workspaceId} /></Tabs.Panel>
        <Tabs.Panel id="spaces" className="pt-4"><SpaceListSettings workspaceId={workspaceId} /></Tabs.Panel>
        <Tabs.Panel id="automations" className="pt-4"><AutomationSettings workspaceId={workspaceId} /></Tabs.Panel>
      </Tabs>
    </div>
  );
}
