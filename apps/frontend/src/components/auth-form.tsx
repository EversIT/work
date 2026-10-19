'use client';

import { Alert, Button, Card, FieldError, Form, Input, Label, TextField } from '@heroui/react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { ApiError } from '../api/fetcher';
import { useLogin, useRegister } from '../api/generated/auth/auth';
import { useGetSystemInfo } from '../api/generated/system/system';
import { body } from '../lib/api';
import { safeNext } from '../lib/redirect';
import { LanguageSwitcher } from './shell/language-switcher';

type Mode = 'login' | 'register';

export function AuthForm({ mode }: { mode: Mode }) {
  const t = useTranslations('auth');
  const router = useRouter();
  const next = safeNext(useSearchParams().get('next'));
  const [error, setError] = useState<string | null>(null);
  const login = useLogin();
  const register = useRegister();
  const info = body(useGetSystemInfo().data);
  const pending = login.isPending || register.isPending;
  const suffix = next !== '/' ? `?next=${encodeURIComponent(next)}` : '';

  function message(e: unknown) {
    if (e instanceof ApiError) {
      if (e.status === 401) return t('errors.invalidCredentials');
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
      if (mode === 'login') {
        await login.mutateAsync({ data: { email, password } });
      } else {
        await register.mutateAsync({
          data: { email, password, first_name: String(form.get('first_name')), last_name: String(form.get('last_name')) },
        });
      }
      router.replace(next);
    } catch (e) {
      setError(message(e));
    }
  }

  return (
    <main className="flex min-h-screen items-start justify-center bg-[#F7F8F9] px-4 pt-[10vh]">
      <Card className="w-full max-w-sm shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f]">
        <Card.Header>
          <div className="mb-3 flex items-center gap-2">
            <span className="flex size-8 items-center justify-center rounded-[4px] bg-gradient-to-br from-[#0C66E4] to-[#1D7AFC] text-base font-bold text-white">w</span>
            <span className="text-lg font-semibold text-[#172B4D]">work</span>
          </div>
          <Card.Title className="text-base">{mode === 'login' ? t('loginTitle') : t('registerTitle')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Form onSubmit={onSubmit} className="flex flex-col gap-4">
            {error && (
              <Alert status="danger">
                <Alert.Content>
                  <Alert.Description>{error}</Alert.Description>
                </Alert.Content>
              </Alert>
            )}
            {mode === 'register' && (
              <>
                <TextField name="first_name" isRequired maxLength={100} autoComplete="given-name" fullWidth>
                  <Label>{t('firstName')}</Label>
                  <Input />
                  <FieldError />
                </TextField>
                <TextField name="last_name" isRequired maxLength={100} autoComplete="family-name" fullWidth>
                  <Label>{t('lastName')}</Label>
                  <Input />
                  <FieldError />
                </TextField>
              </>
            )}
            <TextField name="email" type="email" isRequired autoComplete="email" fullWidth>
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
              autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
              fullWidth
            >
              <Label>{t('password')}</Label>
              <Input />
              {mode === 'register' && <p className="text-xs text-muted">{t('passwordHint')}</p>}
              <FieldError />
            </TextField>
            <Button type="submit" isPending={pending} fullWidth>
              {mode === 'login' ? t('submitLogin') : t('submitRegister')}
            </Button>
            <div className="flex flex-col items-center gap-1 text-sm">
              {mode === 'login' && info?.mail_enabled && (
                <Link className="text-[#0C66E4] hover:underline" href="/forgot-password" data-testid="forgot-link">
                  {t('forgot')}
                </Link>
              )}
              {(mode === 'register' || info?.registration_enabled !== false) && (
                <Link className="text-[#0C66E4] hover:underline" href={`${mode === 'login' ? '/register' : '/login'}${suffix}`}>
                  {mode === 'login' ? t('toRegister') : t('toLogin')}
                </Link>
              )}
            </div>
          </Form>
          <div className="mt-4 max-w-40">
            <LanguageSwitcher />
          </div>
        </Card.Content>
      </Card>
    </main>
  );
}
