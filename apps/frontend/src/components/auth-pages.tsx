'use client';

import { Alert, Button, Card } from '@heroui/react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { confirmPasswordReset, requestPasswordReset } from '../api/generated/auth/auth';
import { errorKind, serverMessage } from '../lib/api';
import { LanguageSwitcher } from './shell/language-switcher';
import { TextInput } from './ui/inputs';

function Shell({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="flex min-h-screen items-start justify-center bg-[#F7F8F9] px-4 pt-[12vh]">
      <Card className="w-full max-w-sm shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f]">
        <Card.Header>
          <div className="mb-2 flex items-center gap-2">
            <span className="flex size-7 items-center justify-center rounded-[4px] bg-gradient-to-br from-[#0C66E4] to-[#1D7AFC] text-sm font-bold text-white">w</span>
            <span className="text-base font-semibold">work</span>
          </div>
          <Card.Title>{title}</Card.Title>
        </Card.Header>
        <Card.Content>
          {children}
          <div className="mt-4 max-w-40"><LanguageSwitcher /></div>
        </Card.Content>
      </Card>
    </main>
  );
}

export function ForgotPassword() {
  const t = useTranslations('auth');
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  return (
    <Shell title={t('forgotTitle')}>
      {sent ? (
        <Alert status="success" data-testid="reset-sent"><Alert.Content><Alert.Description>{t('resetSent')}</Alert.Description></Alert.Content></Alert>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);
            setPending(true);
            try {
              await requestPasswordReset({ email: String(new FormData(e.currentTarget).get('email')) });
              setSent(true);
            } catch (err) {
              setError(errorKind(err) === 'tooMany' ? t('errors.tooMany') : t('errors.generic'));
            } finally {
              setPending(false);
            }
          }}
        >
          <p className="text-sm text-muted">{t('forgotText')}</p>
          {error && <Alert status="danger"><Alert.Content><Alert.Description>{error}</Alert.Description></Alert.Content></Alert>}
          <TextInput label={t('email')} name="email" type="email" required autoFocus />
          <Button type="submit" isPending={pending} fullWidth>{t('sendReset')}</Button>
        </form>
      )}
      <Link href="/login" className="mt-4 block text-center text-sm text-[#0C66E4] hover:underline">{t('backToLogin')}</Link>
    </Shell>
  );
}

export function ResetPassword() {
  const t = useTranslations('auth');
  const token = useSearchParams().get('token') ?? '';
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  return (
    <Shell title={t('resetTitle')}>
      {done ? (
        <Alert status="success" data-testid="reset-done"><Alert.Content><Alert.Description>{t('resetDone')}</Alert.Description></Alert.Content></Alert>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);
            setPending(true);
            try {
              await confirmPasswordReset({ token, new_password: String(new FormData(e.currentTarget).get('password')) });
              setDone(true);
            } catch (err) {
              setError(serverMessage(err) ?? t('errors.generic'));
            } finally {
              setPending(false);
            }
          }}
        >
          {error && <Alert status="danger"><Alert.Content><Alert.Description>{error}</Alert.Description></Alert.Content></Alert>}
          <TextInput label={t('newPassword')} name="password" type="password" required maxLength={128} autoFocus />
          <p className="-mt-2 text-xs text-muted">{t('passwordHint')}</p>
          <Button type="submit" isPending={pending} isDisabled={!token} fullWidth>{t('setPassword')}</Button>
        </form>
      )}
      <Link href="/login" className="mt-4 block text-center text-sm text-[#0C66E4] hover:underline">{t('backToLogin')}</Link>
    </Shell>
  );
}
