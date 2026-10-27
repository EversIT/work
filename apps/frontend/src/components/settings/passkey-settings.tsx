'use client';

import { Button, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  finishPasskeyRegistration,
  passkeyRegistrationOptions,
  useDeletePasskey,
  useListPasskeys,
} from '../../api/generated/auth/auth';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { createPasskey } from '../../lib/passkeys';
import { FormDialog } from '../ui/form-dialog';
import { TextInput } from '../ui/inputs';
import { Panel } from '../ui/settings';

/** Passkeys: sign in with a fingerprint, face or security key instead of a password. */
export function PasskeySettings({
  hasPassword = true,
}: {
  hasPassword?: boolean;
}) {
  const t = useTranslations('passkeys');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const keys = body(useListPasskeys().data) ?? [];
  const remove = useDeletePasskey();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <Panel data-testid="passkey-settings">
      <Panel.Header className="flex-row items-center justify-between">
        <div>
          <Panel.Title>{t('title')}</Panel.Title>
          <Panel.Description>{t('description')}</Panel.Description>
        </div>
        <Button
          size="sm"
          onPress={() => {
            setError(null);
            setOpen(true);
          }}
          data-testid="add-passkey"
        >
          {t('add')}
        </Button>
      </Panel.Header>
      <Panel.Content>
        {keys.length === 0 && <p className="text-sm text-muted">{t('none')}</p>}
        <ul className="divide-y divide-border" data-testid="passkey-list">
          {keys.map((k) => (
            <li key={k.id} className="flex items-center gap-3 py-2 text-sm">
              <div className="min-w-0 flex-1">
                <p className="font-medium">{k.name}</p>
                <p className="text-xs text-muted">
                  {t('created', { date: formatDate(k.created_at, locale) })} ·{' '}
                  {k.last_used_at
                    ? t('lastUsed', {
                        date: formatDate(k.last_used_at, locale, true),
                      })
                    : t('neverUsed')}
                </p>
              </div>
              <Button
                isIconOnly
                size="sm"
                variant="ghost"
                aria-label={t('remove')}
                onPress={async () => {
                  if (!window.confirm(t('confirmRemove'))) return;
                  try {
                    await remove.mutateAsync({ id: k.id });
                    await qc.invalidateQueries();
                  } catch (e) {
                    toast.danger(
                      errorKind(e) === 'conflict'
                        ? t('lastWayIn')
                        : (serverMessage(e) ?? te(errorKind(e))),
                    );
                  }
                }}
              >
                <Trash2 size={14} />
              </Button>
            </li>
          ))}
        </ul>
      </Panel.Content>

      <FormDialog
        isOpen={open}
        onClose={() => setOpen(false)}
        title={t('add')}
        submitLabel={t('create')}
        error={error}
        isPending={busy}
        onSubmit={async (f) => {
          setError(null);
          setBusy(true);
          try {
            const start = body(
              await passkeyRegistrationOptions({
                password: hasPassword
                  ? String(f.get('password') ?? '')
                  : undefined,
              }),
            );
            if (!start) throw new Error('options');
            const response = await createPasskey(start.options);
            await finishPasskeyRegistration({
              challenge_id: start.challenge_id,
              name: String(f.get('name')),
              response,
            });
            await qc.invalidateQueries();
            toast.success(t('added'));
            setOpen(false);
          } catch (e) {
            if (e instanceof DOMException && e.name === 'NotAllowedError')
              setError(t('cancelled'));
            else
              setError(
                errorKind(e) === 'forbidden'
                  ? t('wrongPassword')
                  : errorKind(e) === 'conflict'
                    ? t('tooMany')
                    : (serverMessage(e) ?? te(errorKind(e))),
              );
          } finally {
            setBusy(false);
          }
        }}
      >
        <TextInput
          label={t('name')}
          name="name"
          required
          maxLength={100}
          placeholder={t('namePlaceholder')}
        />
        {hasPassword && (
          <TextInput
            label={t('password')}
            name="password"
            type="password"
            required
          />
        )}
        <p className="text-xs text-muted">{t('hint')}</p>
      </FormDialog>
    </Panel>
  );
}
