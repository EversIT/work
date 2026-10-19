'use client';

import { Label, Switch } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { useCreateSpace } from '../../api/generated/spaces/spaces';
import { errorKind, serverMessage } from '../../lib/api';
import { FormDialog } from '../ui/form-dialog';
import { TextInput } from '../ui/inputs';

export function CreateSpaceDialog({ workspaceId, isOpen, onClose }: { workspaceId: string; isOpen: boolean; onClose: () => void }) {
  const t = useTranslations('shell');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const create = useCreateSpace();
  const [priv, setPriv] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <FormDialog
      isOpen={isOpen}
      onClose={onClose}
      title={t('newSpace')}
      isPending={create.isPending}
      error={error}
      submitLabel={t('create')}
      onSubmit={async (f) => {
        setError(null);
        try {
          await create.mutateAsync({
            id: workspaceId,
            data: { name: String(f.get('name')), access_type: priv ? 'private' : 'public' },
          });
          await qc.invalidateQueries();
          onClose();
        } catch (e) {
          setError(serverMessage(e) ?? te(errorKind(e)));
        }
      }}
    >
      <TextInput label={t('name')} name="name" required maxLength={100} autoFocus />
      <Switch className="flex flex-row items-center gap-2" isSelected={priv} onChange={setPriv}>
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
        <Switch.Content><Label>{t('privateSpace')}</Label></Switch.Content>
      </Switch>
    </FormDialog>
  );
}
