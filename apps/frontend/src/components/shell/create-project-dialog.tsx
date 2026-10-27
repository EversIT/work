'use client';

import { routes } from '../../lib/routes';
import { Label, Switch } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useCreateList } from '../../api/generated/lists/lists';
import { createSpace } from '../../api/generated/spaces/spaces';
import { errorKind, serverMessage } from '../../lib/api';
import { useCardTypes } from '../../lib/hooks';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';
import { useWorkspaceCtx } from './workspace-context';

const NEW_SPACE = '__new';

/**
 * Creates a project in one step, like Jira Cloud. Spaces (the folders projects are grouped in)
 * never have to exist beforehand: with none yet a "General" one is created on the fly, and the
 * "Space" field only appears once there is something to choose.
 */
export function CreateProjectDialog({
  workspaceId,
  onClose,
}: {
  workspaceId: string;
  onClose: () => void;
}) {
  const t = useTranslations('shell');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const ctx = useWorkspaceCtx();
  const create = useCreateList();
  const cardTypes = useCardTypes(workspaceId);
  const [priv, setPriv] = useState(false);
  const [cardType, setCardType] = useState<string | null>(null);
  const [methodology, setMethodology] = useState<string | null>('kanban');
  const [space, setSpace] = useState<string | null>(
    ctx.spaces[0]?.id ?? NEW_SPACE,
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasSpaces = ctx.spaces.length > 0;

  return (
    <FormDialog
      isOpen
      onClose={onClose}
      title={t('newProject')}
      isPending={pending || create.isPending}
      error={error}
      submitLabel={t('create')}
      onSubmit={async (f) => {
        setError(null);
        setPending(true);
        try {
          let spaceId = space && space !== NEW_SPACE ? space : null;
          if (!spaceId) {
            const name = hasSpaces
              ? String(f.get('spaceName') ?? '').trim()
              : t('defaultSpace');
            const made = await createSpace(workspaceId, {
              name: name || t('defaultSpace'),
              access_type: 'public',
            });
            spaceId = made.status === 200 ? made.data.id : null;
          }
          if (!spaceId) throw new Error('space');
          const res = await create.mutateAsync({
            id: spaceId,
            data: {
              name: String(f.get('name')),
              access_type: priv ? 'private' : 'public',
              default_card_type_id: cardType,
              methodology: methodology ?? 'kanban',
            },
          });
          await qc.invalidateQueries();
          onClose();
          if (res.status === 200)
            router.push(
              routes.project(ctx.workspace?.key_prefix ?? '', res.data.slug),
            );
        } catch (e) {
          setError(serverMessage(e) ?? te(errorKind(e)));
        } finally {
          setPending(false);
        }
      }}
    >
      <TextInput
        label={t('projectName')}
        name="name"
        required
        maxLength={100}
      />
      <SimpleSelect
        label={t('template')}
        value={methodology}
        onChange={setMethodology}
        options={[
          { value: 'kanban', label: t('kanban') },
          { value: 'scrum', label: t('scrum') },
        ]}
        testId="list-template"
      />
      {hasSpaces && (
        <SimpleSelect
          label={t('space')}
          value={space}
          onChange={setSpace}
          options={[
            ...ctx.spaces.map((s) => ({ value: s.id, label: s.name })),
            { value: NEW_SPACE, label: t('newSpaceOption') },
          ]}
          testId="project-space"
        />
      )}
      {hasSpaces && space === NEW_SPACE && (
        <TextInput
          label={t('spaceName')}
          name="spaceName"
          required
          maxLength={100}
        />
      )}
      {cardTypes.length > 1 && (
        <SimpleSelect
          label={t('cardType')}
          value={cardType}
          onChange={setCardType}
          options={cardTypes.map((c) => ({ value: c.id, label: c.name }))}
        />
      )}
      <Switch
        className="flex flex-row items-center gap-2"
        isSelected={priv}
        onChange={setPriv}
      >
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
        <Switch.Content>
          <Label>{t('privateList')}</Label>
        </Switch.Content>
      </Switch>
    </FormDialog>
  );
}
