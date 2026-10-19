'use client';

import { Label, Switch } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useCreateList } from '../../api/generated/lists/lists';
import { errorKind, serverMessage } from '../../lib/api';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { useCardTypes } from '../../lib/hooks';

export function CreateListDialog({ workspaceId, spaceId, isOpen, onClose }: { workspaceId: string; spaceId: string; isOpen: boolean; onClose: () => void }) {
  const t = useTranslations('shell');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const create = useCreateList();
  const cardTypes = useCardTypes(workspaceId);
  const [priv, setPriv] = useState(false);
  const [cardType, setCardType] = useState<string | null>(null);
  const [methodology, setMethodology] = useState<string | null>('kanban');
  const [error, setError] = useState<string | null>(null);

  return (
    <FormDialog
      isOpen={isOpen}
      onClose={onClose}
      title={t('newList')}
      isPending={create.isPending}
      error={error}
      submitLabel={t('create')}
      onSubmit={async (f) => {
        setError(null);
        try {
          const res = await create.mutateAsync({
            id: spaceId,
            data: { name: String(f.get('name')), access_type: priv ? 'private' : 'public', default_card_type_id: cardType, methodology: methodology ?? 'kanban' },
          });
          await qc.invalidateQueries();
          onClose();
          if (res.status === 200) router.push(`/workspaces/${workspaceId}/lists/${res.data.slug}`);
        } catch (e) {
          setError(serverMessage(e) ?? te(errorKind(e)));
        }
      }}
    >
      <TextInput label={t('name')} name="name" required maxLength={100} autoFocus />
      <SimpleSelect label={t('template')} value={methodology} onChange={setMethodology} options={[{ value: 'kanban', label: t('kanban') }, { value: 'scrum', label: t('scrum') }]} testId="list-template" />
      {cardTypes.length > 1 && (
        <SimpleSelect label={t('cardType')} value={cardType} onChange={setCardType} options={cardTypes.map((c) => ({ value: c.id, label: c.name }))} />
      )}
      <Switch className="flex flex-row items-center gap-2" isSelected={priv} onChange={setPriv}>
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
        <Switch.Content><Label>{t('privateList')}</Label></Switch.Content>
      </Switch>
    </FormDialog>
  );
}
