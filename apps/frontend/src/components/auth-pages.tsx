'use client';

import { Brand } from './brand';
import { Alert, Button, Card } from '@heroui/react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter, useSearchParams } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import {
  confirmPasswordReset,
  requestPasswordReset,
  verifyEmail,
} from '../api/generated/auth/auth';
import { errorKind, serverMessage } from '../lib/api';
import { refreshAccessToken } from '../api/fetcher';
import { safeNext, takePendingNext } from '../lib/redirect';
import { TextInput } from './ui/inputs';

function Shell({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <main className="flex min-h-screen items-start justify-center bg-surface-secondary px-4 pt-[12vh]">
      <Card className="w-full max-w-sm shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f]">
        <Card.Header>
          <div className="mb-2 flex items-center gap-2">
            <Brand size="sm" nameClassName="text-base font-semibold" />
          </div>
          <Card.Title>{title}</Card.Title>
        </Card.Header>
        <Card.Content>{children}</Card.Content>
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
        <Alert status="success" data-testid="reset-sent">
          <Alert.Content>
            <Alert.Description>{t('resetSent')}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);
            setPending(true);
            try {
              await requestPasswordReset({
                email: String(new FormData(e.currentTarget).get('email')),
              });
              setSent(true);
            } catch (err) {
              setError(
                errorKind(err) === 'tooMany'
                  ? t('errors.tooMany')
                  : t('errors.generic'),
              );
            } finally {
              setPending(false);
            }
          }}
        >
          <p className="text-sm text-muted">{t('forgotText')}</p>
          {error && (
            <Alert status="danger">
              <Alert.Content>
                <Alert.Description>{error}</Alert.Description>
              </Alert.Content>
            </Alert>
          )}
          <TextInput label={t('email')} name="email" type="email" required />
          <Button type="submit" isPending={pending} fullWidth>
            {t('sendReset')}
          </Button>
        </form>
      )}
      <Link
        href="/login"
        className="mt-4 block text-center text-sm text-accent hover:underline"
      >
        {t('backToLogin')}
      </Link>
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
        <Alert status="success" data-testid="reset-done">
          <Alert.Content>
            <Alert.Description>{t('resetDone')}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);
            setPending(true);
            try {
              await confirmPasswordReset({
                token,
                new_password: String(
                  new FormData(e.currentTarget).get('password'),
                ),
              });
              setDone(true);
            } catch (err) {
              setError(serverMessage(err) ?? t('errors.generic'));
            } finally {
              setPending(false);
            }
          }}
        >
          {error && (
            <Alert status="danger">
              <Alert.Content>
                <Alert.Description>{error}</Alert.Description>
              </Alert.Content>
            </Alert>
          )}
          <TextInput
            label={t('newPassword')}
            name="password"
            type="password"
            required
            maxLength={128}
          />
          <p className="-mt-2 text-xs text-muted">{t('passwordHint')}</p>
          <Button
            type="submit"
            isPending={pending}
            isDisabled={!token}
            fullWidth
          >
            {t('setPassword')}
          </Button>
        </form>
      )}
      <Link
        href="/login"
        className="mt-4 block text-center text-sm text-accent hover:underline"
      >
        {t('backToLogin')}
      </Link>
    </Shell>
  );
}

/** Landing page of the confirmation e-mail: redeems the link once and signs the new user in. */
export function VerifyEmail() {
  const t = useTranslations('auth');
  const router = useRouter();
  const token = useSearchParams().get('token') ?? '';
  const [error, setError] = useState<string | null>(
    token ? null : t('errors.invalidLink'),
  );
  // Strict mode runs effects twice in development; the link must only be used once.
  const started = useRef(false);
  useEffect(() => {
    if (!token || started.current) return;
    started.current = true;
    verifyEmail({ token })
      .then(() => router.replace(takePendingNext()))
      .catch((err) => setError(serverMessage(err) ?? t('errors.invalidLink')));
  }, [token, router, t]);
  return (
    <Shell title={t('confirmTitle')}>
      {error ? (
        <Alert status="danger" data-testid="verify-error">
          <Alert.Content>
            <Alert.Description>{error}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <p className="text-sm text-muted" data-testid="verify-pending">
          {t('confirming')}
        </p>
      )}
      <Link
        href="/login"
        className="mt-4 block text-center text-sm text-accent hover:underline"
      >
        {t('backToLogin')}
      </Link>
    </Shell>
  );
}

/** Where the provider's callback sends the browser: the refresh cookie is set, so pick up the session. */
export function SsoComplete() {
  const t = useTranslations('auth');
  const router = useRouter();
  const next = safeNext(useSearchParams().get('next'));
  const [failed, setFailed] = useState(false);
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    refreshAccessToken().then((ok) =>
      ok ? router.replace(next) : setFailed(true),
    );
  }, [next, router]);
  return (
    <Shell title={t('ssoTitle')}>
      {failed ? (
        <Alert status="danger">
          <Alert.Content>
            <Alert.Description>{t('ssoErrors.failed')}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <p className="text-sm text-muted">{t('ssoFinishing')}</p>
      )}
      <Link
        href="/login"
        className="mt-4 block text-center text-sm text-accent hover:underline"
      >
        {t('backToLogin')}
      </Link>
    </Shell>
  );
}
