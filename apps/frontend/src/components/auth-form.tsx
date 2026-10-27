'use client';

import { Brand } from './brand';
import {
  Alert,
  Button,
  Card,
  FieldError,
  Form,
  Input,
  Label,
  TextField,
} from '@heroui/react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { ApiError } from '../api/fetcher';
import {
  passkeyLogin,
  passkeyLoginOptions,
  useLogin,
  useMfaVerify,
  useRegister,
} from '../api/generated/auth/auth';
import { useGetSystemInfo } from '../api/generated/system/system';
import { body } from '../lib/api';
import { getPasskey, passkeysSupported } from '../lib/passkeys';
import { rememberNext, safeNext } from '../lib/redirect';

type Mode = 'login' | 'register';

const SSO_ERRORS = [
  'denied',
  'expired',
  'browser_mismatch',
  'email_unverified',
  'account_exists',
  'registration_closed',
  'locked',
  'identity_in_use',
  'already_linked',
  'saml_invalid',
  'provider_unreachable',
  'too_many',
  'failed',
];

export function AuthForm({ mode }: { mode: Mode }) {
  const t = useTranslations('auth');
  const router = useRouter();
  const next = safeNext(useSearchParams().get('next'));
  const search = useSearchParams();
  const ssoError = search.get('sso_error');
  const [error, setError] = useState<string | null>(
    ssoError
      ? SSO_ERRORS.includes(ssoError)
        ? `sso:${ssoError}`
        : 'sso:failed'
      : null,
  );
  const login = useLogin();
  const register = useRegister();
  const mfaVerify = useMfaVerify();
  const info = body(useGetSystemInfo().data);
  const pending = login.isPending || register.isPending || mfaVerify.isPending;
  // Set after the password was accepted for an account with two-factor authentication.
  const [challenge, setChallenge] = useState<string | null>(null);
  // Set after sign-up when a confirmation e-mail was sent.
  const [confirmSent, setConfirmSent] = useState(false);
  const [passkeyBusy, setPasskeyBusy] = useState(false);

  /** Usernameless sign-in: the authenticator offers its passkeys; no e-mail or password is typed. */
  async function signInWithPasskey() {
    setError(null);
    setPasskeyBusy(true);
    try {
      const start = body(await passkeyLoginOptions());
      if (!start) throw new Error('options');
      const cred = await getPasskey(start.options);
      await passkeyLogin({
        challenge_id: start.challenge_id,
        id: cred.id,
        response: cred.response,
      });
      router.replace(next);
    } catch (e) {
      // Cancelling the browser dialog is not an error worth a message.
      setError(
        e instanceof ApiError
          ? t('errors.passkeyFailed')
          : e instanceof DOMException && e.name === 'NotAllowedError'
            ? null
            : t('errors.passkeyFailed'),
      );
    } finally {
      setPasskeyBusy(false);
    }
  }
  const suffix = next !== '/' ? `?next=${encodeURIComponent(next)}` : '';

  function message(e: unknown) {
    if (e instanceof ApiError) {
      if (e.status === 401)
        return challenge
          ? t('errors.invalidCode')
          : t('errors.invalidCredentials');
      if (
        e.status === 400 &&
        typeof (e.body as { error?: unknown })?.error === 'string'
      )
        return (e.body as { error: string }).error;
      if (e.status === 409) return t('errors.emailTaken');
      if (e.status === 429) return t('errors.tooMany');
      if (e.status === 403) return t('errors.registrationClosed');
    }
    return t('errors.generic');
  }

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    const form = new FormData(event.currentTarget);
    const email = String(form.get('email'));
    const password = String(form.get('password'));
    try {
      if (challenge) {
        await mfaVerify.mutateAsync({
          data: { mfa_token: challenge, code: String(form.get('code')) },
        });
      } else if (mode === 'login') {
        const out = body(
          await login.mutateAsync({ data: { email, password } }),
        );
        if (out?.mfa_required && out.mfa_token) {
          setChallenge(out.mfa_token);
          return;
        }
      } else {
        const out = body(
          await register.mutateAsync({
            data: {
              email,
              password,
              first_name: String(form.get('first_name')),
              last_name: String(form.get('last_name')),
            },
          }),
        );
        if (out?.verification_required) {
          rememberNext(next);
          setConfirmSent(true);
          return;
        }
      }
      router.replace(next);
    } catch (e) {
      setError(message(e));
    }
  }

  return (
    <main className="flex min-h-screen items-start justify-center bg-surface-secondary px-4 pt-[10vh]">
      <Card className="w-full max-w-sm shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f]">
        <Card.Header>
          <div className="mb-3 flex items-center gap-2">
            <Brand
              size="md"
              nameClassName="text-lg font-semibold text-foreground"
            />
          </div>
          <Card.Title className="text-base">
            {challenge
              ? t('mfaTitle')
              : confirmSent
                ? t('confirmTitle')
                : mode === 'login'
                  ? t('loginTitle')
                  : t('registerTitle')}
          </Card.Title>
        </Card.Header>
        <Card.Content>
          {confirmSent ? (
            <div className="flex flex-col gap-4">
              <Alert status="success" data-testid="confirm-sent">
                <Alert.Content>
                  <Alert.Description>{t('confirmSent')}</Alert.Description>
                </Alert.Content>
              </Alert>
              <Link
                className="text-center text-sm text-accent hover:underline"
                href={`/login${suffix}`}
              >
                {t('backToLogin')}
              </Link>
            </div>
          ) : challenge ? (
            <Form onSubmit={onSubmit} className="flex flex-col gap-4">
              {error && (
                <Alert status="danger">
                  <Alert.Content>
                    <Alert.Description>
                      {error.startsWith('sso:')
                        ? t(`ssoErrors.${error.slice(4)}`)
                        : error}
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
              )}
              <p className="text-sm text-muted">{t('mfaText')}</p>
              <TextField
                name="code"
                isRequired
                autoComplete="one-time-code"
                inputMode="text"
                maxLength={32}
                fullWidth
              >
                <Label>{t('mfaCode')}</Label>
                <Input data-testid="mfa-code" />
                <FieldError />
              </TextField>
              <Button
                type="submit"
                isPending={pending}
                fullWidth
                data-testid="mfa-submit"
              >
                {t('mfaSubmit')}
              </Button>
              <Button
                type="button"

                onPress={() => {
                  setChallenge(null);
                  setError(null);
                }}
                variant="ghost"
                size="sm"
              >
                {t('backToLogin')}
              </Button>
            </Form>
          ) : (
            <Form onSubmit={onSubmit} className="flex flex-col gap-4">
              {error && (
                <Alert status="danger">
                  <Alert.Content>
                    <Alert.Description>
                      {error.startsWith('sso:')
                        ? t(`ssoErrors.${error.slice(4)}`)
                        : error}
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
              )}
              {mode === 'register' && (
                <>
                  <TextField
                    name="first_name"
                    isRequired
                    maxLength={100}
                    autoComplete="given-name"
                    fullWidth
                  >
                    <Label>{t('firstName')}</Label>
                    <Input />
                    <FieldError />
                  </TextField>
                  <TextField
                    name="last_name"
                    isRequired
                    maxLength={100}
                    autoComplete="family-name"
                    fullWidth
                  >
                    <Label>{t('lastName')}</Label>
                    <Input />
                    <FieldError />
                  </TextField>
                </>
              )}
              <TextField
                name="email"
                type="email"
                isRequired
                autoComplete="email"
                fullWidth
              >
                <Label>{t('email')}</Label>
                <Input />
                <FieldError />
              </TextField>
              <TextField
                name="password"
                type="password"
                isRequired
                minLength={mode === 'register' ? 12 : undefined}
                maxLength={128}
                autoComplete={
                  mode === 'login' ? 'current-password' : 'new-password'
                }
                fullWidth
              >
                <Label>{t('password')}</Label>
                <Input />
                {mode === 'register' && (
                  <p className="text-xs text-muted">{t('passwordHint')}</p>
                )}
                <FieldError />
              </TextField>
              <Button type="submit" isPending={pending} fullWidth>
                {mode === 'login' ? t('submitLogin') : t('submitRegister')}
              </Button>
              {mode === 'login' &&
                info?.passkeys_available &&
                passkeysSupported() && (
                  <Button
                    type="button"
                    variant="secondary"
                    fullWidth
                    isPending={passkeyBusy}
                    onPress={signInWithPasskey}
                    data-testid="passkey-login"
                  >
                    {t('passkeyLogin')}
                  </Button>
                )}
              {mode === 'login' && (info?.sso_providers?.length ?? 0) > 0 && (
                <div className="flex flex-col gap-2" data-testid="sso-buttons">
                  <p className="text-center text-xs text-muted">{t('ssoOr')}</p>
                  {info?.sso_providers.map((p) => (
                    // A full navigation, not a fetch: the provider's page must replace this one.
                    <a
                      key={p}
                      href={`/api/v1/auth/sso/${p}/start${next !== '/' ? `?next=${encodeURIComponent(next)}` : ''}`}
                      data-testid={`sso-${p}`}
                      className="rounded-sm border border-border px-3 py-2 text-center text-sm font-medium text-foreground hover:bg-surface-tertiary"
                    >
                      {t('ssoWith', {
                        provider:
                          p === 'oidc'
                            ? (info?.sso_oidc_name ?? 'OpenID Connect')
                            : t(`ssoProviders.${p}`),
                      })}
                    </a>
                  ))}
                </div>
              )}
              <div className="flex flex-col items-center gap-1 text-sm">
                {mode === 'login' && info?.mail_enabled && (
                  <Link
                    className="text-accent hover:underline"
                    href="/forgot-password"
                    data-testid="forgot-link"
                  >
                    {t('forgot')}
                  </Link>
                )}
                {(mode === 'register' ||
                  info?.registration_enabled !== false) && (
                  <Link
                    className="text-accent hover:underline"
                    href={`${mode === 'login' ? '/register' : '/login'}${suffix}`}
                  >
                    {mode === 'login' ? t('toRegister') : t('toLogin')}
                  </Link>
                )}
              </div>
            </Form>
          )}
        </Card.Content>
      </Card>
    </main>
  );
}
