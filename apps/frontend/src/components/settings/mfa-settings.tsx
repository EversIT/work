'use client';

import { Button, Chip, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import QRCode from 'qrcode';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  useDisableMfa,
  useEnableMfa,
  useRegenerateRecoveryCodes,
  useSetupMfa,
} from '../../api/generated/auth/auth';
import { body, errorKind, serverMessage } from '../../lib/api';
import { FormDialog } from '../ui/form-dialog';
import { TextInput } from '../ui/inputs';
import { Panel } from '../ui/settings';

type Dialog = 'setup' | 'disable' | 'recovery' | null;

/** Two-factor authentication with an authenticator app (TOTP) and single-use recovery codes. */
export function MfaSettings({ enabled }: { enabled: boolean }) {
  const t = useTranslations('mfa');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const setup = useSetupMfa();
  const enable = useEnableMfa();
  const disable = useDisableMfa();
  const regenerate = useRegenerateRecoveryCodes();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [error, setError] = useState<string | null>(null);
  // Enrolment: `secret` is set once the password was accepted, `codes` once the first code was.
  const [secret, setSecret] = useState<{ secret: string; qr: string } | null>(
    null,
  );
  const [codes, setCodes] = useState<string[] | null>(null);

  function close() {
    setDialog(null);
    setError(null);
    setSecret(null);
    setCodes(null);
  }

  const fail = (e: unknown) =>
    setError(
      errorKind(e) === 'forbidden'
        ? t('wrongCredentials')
        : (serverMessage(e) ?? te(errorKind(e))),
    );

  return (
    <Panel data-testid="mfa-settings">
      <Panel.Header className="flex-row items-center justify-between">
        <div>
          <Panel.Title>{t('title')}</Panel.Title>
          <Panel.Description>{t('description')}</Panel.Description>
        </div>
        <Chip size="sm" color={enabled ? 'success' : 'default'}>
          <Chip.Label>{enabled ? t('on') : t('off')}</Chip.Label>
        </Chip>
      </Panel.Header>
      <Panel.Content className="flex flex-wrap gap-2">
        {enabled ? (
          <>
            <Button
              variant="secondary"
              onPress={() => setDialog('recovery')}
              data-testid="mfa-recovery"
            >
              {t('newRecoveryCodes')}
            </Button>
            <Button
              variant="danger-soft"
              onPress={() => setDialog('disable')}
              data-testid="mfa-disable"
            >
              {t('disable')}
            </Button>
          </>
        ) : (
          <Button onPress={() => setDialog('setup')} data-testid="mfa-enable">
            {t('enable')}
          </Button>
        )}
      </Panel.Content>

      <FormDialog
        isOpen={dialog === 'setup'}
        onClose={close}
        title={t('setupTitle')}
        submitLabel={codes ? t('done') : secret ? t('confirm') : t('continue')}
        error={error}
        isPending={setup.isPending || enable.isPending}
        onSubmit={async (f) => {
          setError(null);
          try {
            if (codes) {
              close();
            } else if (secret) {
              const r = body(
                await enable.mutateAsync({
                  data: { code: String(f.get('code')) },
                }),
              );
              setCodes(r?.recovery_codes ?? []);
              await qc.invalidateQueries();
            } else {
              const r = body(
                await setup.mutateAsync({
                  data: { password: String(f.get('password')) },
                }),
              );
              // Drawn in the browser: the secret never goes to a third-party QR service.
              if (r)
                setSecret({
                  secret: r.secret,
                  qr: await QRCode.toDataURL(r.otpauth_uri, {
                    margin: 1,
                    width: 192,
                  }),
                });
            }
          } catch (e) {
            fail(e);
          }
        }}
      >
        {codes ? (
          <RecoveryCodes codes={codes} />
        ) : secret ? (
          <>
            <p className="text-sm">{t('scan')}</p>
            {/* A data: URL drawn locally; next/image would only add a loader for it. */}
            {/* oxlint-disable-next-line next/no-img-element */}
            <img
              src={secret.qr}
              alt={t('qrAlt')}
              width={192}
              height={192}
              className="self-center rounded border border-border"
            />
            <p className="text-xs text-muted">{t('manual')}</p>
            <code
              className="break-all rounded bg-surface-tertiary p-2 text-sm"
              data-testid="mfa-secret"
            >
              {secret.secret}
            </code>
            <TextInput label={t('code')} name="code" required maxLength={8} />
          </>
        ) : (
          <>
            <p className="text-sm">{t('passwordFirst')}</p>
            <TextInput
              label={t('password')}
              name="password"
              type="password"
              required
            />
          </>
        )}
      </FormDialog>

      <FormDialog
        isOpen={dialog === 'disable'}
        onClose={close}
        title={t('disable')}
        submitLabel={t('disable')}
        danger
        error={error}
        isPending={disable.isPending}
        onSubmit={async (f) => {
          setError(null);
          try {
            await disable.mutateAsync({
              data: {
                password: String(f.get('password')),
                code: String(f.get('code')),
              },
            });
            await qc.invalidateQueries();
            toast.success(t('disabled'));
            close();
          } catch (e) {
            fail(e);
          }
        }}
      >
        <p className="text-sm">{t('disableConfirm')}</p>
        <TextInput
          label={t('password')}
          name="password"
          type="password"
          required
        />
        <TextInput
          label={t('codeOrRecovery')}
          name="code"
          required
          maxLength={32}
        />
      </FormDialog>

      <FormDialog
        isOpen={dialog === 'recovery'}
        onClose={close}
        title={t('newRecoveryCodes')}
        submitLabel={codes ? t('done') : t('newRecoveryCodes')}
        error={error}
        isPending={regenerate.isPending}
        onSubmit={async (f) => {
          setError(null);
          try {
            if (codes) return close();
            const r = body(
              await regenerate.mutateAsync({
                data: {
                  password: String(f.get('password')),
                  code: String(f.get('code')),
                },
              }),
            );
            setCodes(r?.recovery_codes ?? []);
          } catch (e) {
            fail(e);
          }
        }}
      >
        {codes ? (
          <RecoveryCodes codes={codes} />
        ) : (
          <>
            <p className="text-sm">{t('recoveryReplace')}</p>
            <TextInput
              label={t('password')}
              name="password"
              type="password"
              required
            />
            <TextInput
              label={t('codeOrRecovery')}
              name="code"
              required
              maxLength={32}
            />
          </>
        )}
      </FormDialog>
    </Panel>
  );
}

function RecoveryCodes({ codes }: { codes: string[] }) {
  const t = useTranslations('mfa');
  return (
    <>
      <p className="text-sm">{t('recoveryHint')}</p>
      <ul
        className="grid grid-cols-2 gap-1 rounded bg-surface-tertiary p-3 font-mono text-sm"
        data-testid="recovery-codes"
      >
        {codes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        className="self-start"
        onPress={() =>
          navigator.clipboard
            ?.writeText(codes.join('\n'))
            .then(() => toast.success(t('copied')))
        }
      >
        {t('copy')}
      </Button>
    </>
  );
}
