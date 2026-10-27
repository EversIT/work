'use client';

import { Alert, Button, toast } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { useChangePassword } from '../../api/generated/auth/auth';
import type { Me } from '../../api/generated/model';
import { errorKind, serverMessage } from '../../lib/api';
import { TextInput } from '../ui/inputs';
import { Panel, SettingsRow } from '../ui/settings';
import { MfaSettings } from './mfa-settings';
import { PasskeySettings } from './passkey-settings';
import { SsoSettings } from './sso-settings';

/** How you prove it is you: password, second factor, passkeys and linked sign-in providers. */
export function AccountSecurity({
  me,
  providers,
  mfaAvailable,
}: {
  me: Me;
  providers: string[];
  mfaAvailable: boolean;
}) {
  const t = useTranslations('settings');
  const te = useTranslations('errors');
  const changePw = useChangePassword();
  const [pwError, setPwError] = useState<string | null>(null);

  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <Panel.Header>
          <Panel.Title>{t('password')}</Panel.Title>
        </Panel.Header>
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setPwError(null);
            const form = e.currentTarget;
            const f = new FormData(form);
            try {
              await changePw.mutateAsync({
                data: {
                  current_password: String(f.get('current')),
                  new_password: String(f.get('next')),
                },
              });
              form.reset();
              toast.success(t('passwordChanged'));
            } catch (err) {
              setPwError(
                errorKind(err) === 'forbidden'
                  ? t('wrongPassword')
                  : (serverMessage(err) ?? te(errorKind(err))),
              );
            }
          }}
        >
          {pwError && (
            <div className="px-5 pt-4">
              <Alert status="danger">
                <Alert.Content>
                  <Alert.Description>{pwError}</Alert.Description>
                </Alert.Content>
              </Alert>
            </div>
          )}
          <SettingsRow label={t('currentPassword')}>
            <TextInput
              label={t('currentPassword')}
              name="current"
              type="password"
              required
            />
          </SettingsRow>
          <SettingsRow label={t('newPassword')} help={t('passwordHint')}>
            <TextInput
              label={t('newPassword')}
              name="next"
              type="password"
              required
              maxLength={128}
            />
          </SettingsRow>
          <div className="flex justify-end border-t border-separator px-5 py-3">
            <Button
              type="submit"
              isPending={changePw.isPending}
              data-testid="change-password"
            >
              {t('changePassword')}
            </Button>
          </div>
        </form>
      </Panel>
      {(mfaAvailable || me.mfa_enabled) && (
        <MfaSettings enabled={me.mfa_enabled} />
      )}
      <PasskeySettings />
      <SsoSettings providers={providers} />
    </div>
  );
}
