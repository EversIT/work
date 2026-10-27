'use client';

import { toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useLocale, useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import {
  useDeleteWorkspace,
  useUpdateWorkspace,
} from '../../api/generated/workspaces/workspaces';
import { errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { useWorkspace } from '../../lib/hooks';
import { Button } from '@heroui/react';
import { TextInput } from '../ui/inputs';
import {
  ConfirmByTyping,
  Panel,
  SettingsRow,
  UnsavedBar,
} from '../ui/settings';

/** Name and key of the workspace, and the one irreversible action. */
export function WorkspaceDetails({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('wsettings');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const router = useRouter();
  const ws = useWorkspace(workspaceId);
  const rename = useUpdateWorkspace();
  const del = useDeleteWorkspace();
  const [name, setName] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  if (!ws) return null;

  const dirty = name !== null && name.trim() !== '' && name !== ws.name;
  const save = async () => {
    try {
      await rename.mutateAsync({ id: workspaceId, data: { name: name ?? '' } });
      await qc.invalidateQueries();
      setName(null);
      toast.success(t('saved'));
    } catch (e) {
      toast.danger(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <SettingsRow label={t('name')} help={t('nameHelp')}>
          <TextInput
            label={t('name')}
            value={name ?? ws.name}
            onChange={setName}
            maxLength={100}
          />
        </SettingsRow>
        <SettingsRow
          label={t('key')}
          help={t('keyHelp', { key: ws.key_prefix })}
        >
          <code
            className="inline-block rounded-md bg-default px-2.5 py-1 font-mono text-sm"
            data-testid="project-key"
          >
            {ws.key_prefix}
          </code>
        </SettingsRow>
        <SettingsRow label={t('created')}>
          <p className="text-sm text-foreground">
            {formatDate(ws.created_at, locale, true)}
          </p>
        </SettingsRow>
      </Panel>

      <Panel tone="danger" testId="danger-zone">
        <Panel.Header className="border-danger/30">
          <div>
            <Panel.Title className="text-danger">
              {t('dangerTitle')}
            </Panel.Title>
          </div>
        </Panel.Header>
        <SettingsRow label={t('deleteTitle')} help={t('deleteHelp')}>
          <Button
            variant="danger-soft"
            onPress={() => setConfirming(true)}
            data-testid="delete-workspace"
          >
            {t('deleteButton')}
          </Button>
        </SettingsRow>
      </Panel>

      <UnsavedBar
        show={dirty}
        pending={rename.isPending}
        onSave={save}
        onDiscard={() => setName(null)}
      />
      <ConfirmByTyping
        isOpen={confirming}
        onClose={() => setConfirming(false)}
        title={t('confirmTitle', { name: ws.name })}
        help={t('confirmHelp')}
        expected={ws.name}
        confirmLabel={t('deleteButton')}
        onConfirm={async () => {
          await del.mutateAsync({ id: workspaceId });
          await qc.invalidateQueries();
          router.replace('/');
        }}
      />
    </div>
  );
}
