'use client';

import { Button, Card, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'next/navigation';
import { useTranslations } from 'next-intl';
import { useRef, useState } from 'react';
import {
  deleteFavicon,
  updateBranding,
  useGetBranding,
} from '../../api/generated/admin/admin';
import { apiFetch } from '../../api/fetcher';
import { body, errorKind, serverMessage } from '../../lib/api';
import { accentVars } from '../../lib/theme';
import { TextAreaInput, TextInput } from '../ui/inputs';

/** Name, description, accent colour and favicon of the instance; changes show on the next page load. */
export function Appearance() {
  const t = useTranslations('admin');
  const te = useTranslations('errors');
  const router = useRouter();
  const qc = useQueryClient();
  const branding = body(useGetBranding().data);
  const [title, setTitle] = useState<string | null>(null);
  const [description, setDescription] = useState<string | null>(null);
  const [accent, setAccent] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  if (!branding) return null;

  const current = {
    title: title ?? branding.title,
    description: description ?? branding.description,
    accent: accent ?? branding.accent,
  };
  const preview = accentVars(current.accent);
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));
  // The title, description, favicon and colours are read by the server when a page is rendered.
  const refresh = async () => {
    await qc.invalidateQueries();
    router.refresh();
  };

  return (
    <div className="grid gap-4 lg:grid-cols-2" data-testid="admin-appearance">
      <Card>
        <Card.Header>
          <Card.Title>{t('appearance.identity')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <form
            className="flex flex-col gap-3"
            onSubmit={async (e) => {
              e.preventDefault();
              try {
                await updateBranding(current);
                setTitle(null);
                setDescription(null);
                setAccent(null);
                toast.success(t('saved'));
                await refresh();
              } catch (err) {
                fail(err);
              }
            }}
          >
            <TextInput
              label={t('appearance.title')}
              value={current.title}
              onChange={setTitle}
              required
              maxLength={60}
            />
            <TextAreaInput
              label={t('appearance.description')}
              value={current.description}
              onChange={setDescription}
              rows={3}
              maxLength={300}
            />
            <div className="flex items-end gap-3">
              <label className="flex flex-col gap-1 text-sm font-medium">
                {t('appearance.accent')}
                <input
                  type="color"
                  value={current.accent.toLowerCase()}
                  onChange={(e) => setAccent(e.target.value)}
                  className="h-9 w-16 cursor-pointer rounded border border-field-border bg-surface"
                  data-testid="accent-input"
                />
              </label>
              <code className="pb-2 text-sm">
                {current.accent.toUpperCase()}
              </code>
              {accent !== null && (
                <Button
                  size="sm"
                  variant="tertiary"
                  onPress={() => setAccent(null)}
                >
                  {t('appearance.reset')}
                </Button>
              )}
            </div>
            <Button
              type="submit"
              className="self-start"
              data-testid="save-branding"
            >
              {t('save')}
            </Button>
          </form>
        </Card.Content>
      </Card>

      <div className="flex flex-col gap-4">
        <Card>
          <Card.Header>
            <Card.Title>{t('appearance.favicon')}</Card.Title>
            <Card.Description>{t('appearance.faviconHint')}</Card.Description>
          </Card.Header>
          <Card.Content className="flex items-center gap-4">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={
                branding.has_favicon
                  ? `/api/v1/branding/favicon?v=${branding.version}`
                  : '/icon.svg'
              }
              alt={t('appearance.currentFavicon')}
              width={48}
              height={48}
              className="size-12 rounded border border-border"
              data-testid="favicon-preview"
            />
            <input
              ref={fileRef}
              type="file"
              accept="image/png,image/jpeg,image/webp,image/x-icon,image/svg+xml"
              className="hidden"
              data-testid="favicon-input"
              onChange={async (e) => {
                const file = e.target.files?.[0];
                e.target.value = '';
                if (!file) return;
                try {
                  await apiFetch('/api/v1/admin/branding/favicon', {
                    method: 'PUT',
                    body: file,
                    headers: { 'Content-Type': 'application/octet-stream' },
                  });
                  toast.success(t('saved'));
                  await refresh();
                } catch (err) {
                  fail(err);
                }
              }}
            />
            <Button size="sm" onPress={() => fileRef.current?.click()}>
              {t('appearance.upload')}
            </Button>
            {branding.has_favicon && (
              <Button
                size="sm"
                variant="tertiary"
                onPress={async () => {
                  try {
                    await deleteFavicon();
                    await refresh();
                  } catch (err) {
                    fail(err);
                  }
                }}
              >
                {t('appearance.useDefault')}
              </Button>
            )}
          </Card.Content>
        </Card>

        <Card>
          <Card.Header>
            <Card.Title>{t('appearance.preview')}</Card.Title>
          </Card.Header>
          <Card.Content>
            <div
              style={preview ?? undefined}
              className="flex flex-col gap-3 rounded border border-border p-4"
              data-testid="theme-preview"
            >
              <p className="text-sm font-semibold">{current.title}</p>
              <p className="text-xs text-muted">{current.description}</p>
              <div className="flex items-center gap-2">
                <Button type="button" variant="primary" size="sm">
                  {t('appearance.primary')}
                </Button>
                <span className="rounded-sm bg-accent-soft px-3 py-1.5 text-sm font-medium text-accent-soft-foreground">
                  {t('appearance.soft')}
                </span>
                <a
                  className="text-sm text-accent underline"
                  href="#preview"
                  onClick={(e) => e.preventDefault()}
                >
                  {t('appearance.link')}
                </a>
              </div>
            </div>
          </Card.Content>
        </Card>
      </div>
    </div>
  );
}
