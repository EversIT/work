'use client';

import { Button, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { setAccessToken } from '../../api/fetcher';
import { useDeleteMe, useUpdateMe } from '../../api/generated/auth/auth';
import type { Me } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { FormDialog } from '../ui/form-dialog';
import { TextInput } from '../ui/inputs';
import { Panel, SettingsRow } from '../ui/settings';

/** Your name and e-mail address, and deleting the account. */
export function AccountProfile({ me }: { me: Me }) {
  const t = useTranslations('settings');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const router = useRouter();
  const updateMe = useUpdateMe();
  const deleteMe = useDeleteMe();
  const [del, setDel] = useState(false);
  const [delError, setDelError] = useState<string | null>(null);

  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            try {
              await updateMe.mutateAsync({
                data: {
                  first_name: String(f.get('first')),
                  last_name: String(f.get('last')),
                },
              });
              await qc.invalidateQueries();
              toast.success(t('saved'));
            } catch (err) {
              toast.danger(serverMessage(err) ?? te(errorKind(err)));
            }
          }}
        >
          <SettingsRow label={t('firstName')}>
            <TextInput
              label={t('firstName')}
              name="first"
              defaultValue={me.first_name}
              required
              maxLength={100}
            />
          </SettingsRow>
          <SettingsRow label={t('lastName')}>
            <TextInput
              label={t('lastName')}
              name="last"
              defaultValue={me.last_name}
              required
              maxLength={100}
            />
          </SettingsRow>
          <SettingsRow label={t('email')}>
            <TextInput label={t('email')} value={me.email} type="email" />
          </SettingsRow>
          <div className="flex justify-end border-t border-separator px-5 py-3">
            <Button type="submit" isPending={updateMe.isPending}>
              {t('save')}
            </Button>
          </div>
        </form>
      </Panel>

      <Panel tone="danger">
        <Panel.Header className="border-danger/30">
          <Panel.Title className="text-danger">{t('dangerZone')}</Panel.Title>
        </Panel.Header>
        <SettingsRow label={t('deleteAccount')} help={t('deleteExplain')}>
          <Button
            variant="danger-soft"
            onPress={() => setDel(true)}
            data-testid="delete-account"
          >
            {t('deleteAccount')}
          </Button>
        </SettingsRow>
      </Panel>

      <FormDialog
        isOpen={del}
        onClose={() => setDel(false)}
        title={t('deleteAccount')}
        submitLabel={t('deleteAccount')}
        danger
        error={delError}
        isPending={deleteMe.isPending}
        onSubmit={async (f) => {
          setDelError(null);
          try {
            await deleteMe.mutateAsync({
              data: { password: String(f.get('password')) },
            });
            setAccessToken(null);
            qc.clear();
            router.replace('/login');
          } catch (err) {
            const k = errorKind(err);
            setDelError(
              k === 'forbidden'
                ? t('wrongPassword')
                : k === 'conflict'
                  ? t('deleteBlocked')
                  : te(k),
            );
          }
        }}
      >
        <p className="text-sm">{t('deleteConfirm')}</p>
        <TextInput
          label={t('currentPassword')}
          name="password"
          type="password"
          required
        />
      </FormDialog>
    </div>
  );
}
