'use client';

import { routes } from '../../lib/routes';
import { Button, Chip, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useDeleteList, useUpdateList } from '../../api/generated/lists/lists';
import type { ListDetail } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { useCardTypes, useWorkspace } from '../../lib/hooks';
import { SimpleSelect, TextInput } from '../ui/inputs';
import {
  ConfirmByTyping,
  EditScope,
  Panel,
  SettingsRow,
  UnsavedBar,
} from '../ui/settings';

/** Name, template and default issue type of a project, and deleting it. */
export function ProjectDetails({
  list,
  canEdit,
}: {
  list: ListDetail;
  canEdit: boolean;
}) {
  const t = useTranslations('psettings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const types = useCardTypes(list.workspace_id);
  const ws = useWorkspace(list.workspace_id);
  const update = useUpdateList();
  const del = useDeleteList();
  const [name, setName] = useState<string | null>(null);
  const [defType, setDefType] = useState<string | null | undefined>(undefined);
  const [confirming, setConfirming] = useState(false);

  const current =
    defType === undefined ? (list.default_card_type_id ?? null) : defType;
  const dirty =
    (name !== null && name.trim() !== '' && name !== list.name) ||
    (defType !== undefined && defType !== (list.default_card_type_id ?? null));

  const save = async () => {
    try {
      await update.mutateAsync({
        id: list.id,
        data: {
          ...(name !== null && name.trim() ? { name: name.trim() } : {}),
          ...(defType ? { default_card_type_id: defType } : {}),
        },
      });
      await qc.invalidateQueries();
      setName(null);
      setDefType(undefined);
      toast.success(t('saved'));
    } catch (e) {
      toast.danger(serverMessage(e) ?? te(errorKind(e)));
    }
  };

  return (
    <EditScope canEdit={canEdit}>
      <div className="flex flex-col gap-6">
        <Panel>
          <SettingsRow label={t('name')} help={t('nameHelp')}>
            <TextInput
              label={t('name')}
              value={name ?? list.name}
              onChange={setName}
              maxLength={100}
            />
          </SettingsRow>
          <SettingsRow label={t('template')} help={t('templateHelp')}>
            <Chip size="md" data-testid="project-template">
              <Chip.Label>
                {list.methodology === 'scrum' ? t('scrum') : t('kanban')}
              </Chip.Label>
            </Chip>
          </SettingsRow>
          <SettingsRow label={t('defaultType')} help={t('defaultTypeHelp')}>
            <SimpleSelect
              label={t('defaultType')}
              value={current}
              onChange={(v) => setDefType(v)}
              options={types.map((c) => ({ value: c.id, label: c.name }))}
            />
          </SettingsRow>
          <SettingsRow label={t('address')} help={t('addressHelp')}>
            <code className="inline-block rounded-md bg-default px-2.5 py-1 font-mono text-sm">
              {list.slug}
            </code>
          </SettingsRow>
        </Panel>

        {canEdit && (
          <Panel tone="danger">
            <Panel.Header className="border-danger/30">
              <Panel.Title className="text-danger">
                {t('dangerTitle')}
              </Panel.Title>
            </Panel.Header>
            <SettingsRow label={t('deleteTitle')} help={t('deleteHelp')}>
              <Button
                variant="danger-soft"
                onPress={() => setConfirming(true)}
                data-testid="delete-project"
              >
                {t('deleteButton')}
              </Button>
            </SettingsRow>
          </Panel>
        )}

        <UnsavedBar
          show={canEdit && dirty}
          pending={update.isPending}
          onSave={save}
          onDiscard={() => {
            setName(null);
            setDefType(undefined);
          }}
        />
        <ConfirmByTyping
          isOpen={confirming}
          onClose={() => setConfirming(false)}
          title={t('confirmTitle', { name: list.name })}
          help={t('confirmHelp')}
          expected={list.name}
          confirmLabel={t('deleteButton')}
          onConfirm={async () => {
            await del.mutateAsync({ id: list.id });
            await qc.invalidateQueries();
            router.replace(routes.workspace(ws?.key_prefix ?? ''));
          }}
        />
      </div>
    </EditScope>
  );
}
