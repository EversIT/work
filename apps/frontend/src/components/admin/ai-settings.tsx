'use client';

import { Button, Chip, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  removeAiSettings,
  setAiSettings,
  testAiSettings,
  useGetAiSettings,
} from '../../api/generated/ai/ai';
import { body, errorKind, serverMessage } from '../../lib/api';
import { Panel } from '../ui/settings';
import { TextInput } from '../ui/inputs';

/**
 * The self-hosted language model behind the AI features. Without one the AI features are off
 * everywhere in the app; saving an address switches them on for everybody at once. There is no
 * API key: the model runs on the operator's own infrastructure.
 */
export function AiSettings() {
  const t = useTranslations('admin');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const settings = body(useGetAiSettings().data);
  const [url, setUrl] = useState<string | null>(null);
  const [name, setName] = useState<string | null>(null);
  const [busy, setBusy] = useState<'test' | 'save' | 'remove' | null>(null);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(
    null,
  );
  if (!settings) return null;

  const form = {
    model_url: url ?? settings.model_url ?? '',
    model_name: name ?? settings.model_name ?? '',
  };
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));
  const reset = () => {
    setUrl(null);
    setName(null);
    setResult(null);
  };
  const refresh = () => qc.invalidateQueries();

  const test = async () => {
    setBusy('test');
    setResult(null);
    try {
      const res = await testAiSettings(form);
      if (res.status === 200)
        setResult({
          ok: res.data.ok,
          text: res.data.ok ? t('ai.testOk') : res.data.message,
        });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };
  const save = async () => {
    setBusy('save');
    try {
      await setAiSettings(form);
      reset();
      toast.success(t('saved'));
      await refresh();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };
  const remove = async () => {
    setBusy('remove');
    try {
      await removeAiSettings();
      reset();
      toast.success(t('saved'));
      await refresh();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="grid gap-4 lg:grid-cols-2" data-testid="admin-ai">
      <Panel>
        <Panel.Header className="flex-row items-start justify-between gap-3">
          <div>
            <Panel.Title>{t('ai.title')}</Panel.Title>
            <Panel.Description>{t('ai.help')}</Panel.Description>
          </div>
          <Chip
            color={settings.enabled ? 'success' : 'default'}
            data-testid="ai-state"
          >
            <Chip.Label>{settings.enabled ? t('on') : t('off')}</Chip.Label>
          </Chip>
        </Panel.Header>
        <Panel.Content>
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <TextInput
              label={t('ai.url')}
              value={form.model_url}
              onChange={(v) => {
                setUrl(v);
                setResult(null);
              }}
              placeholder="http://model:8081"
              required
              maxLength={500}
            />
            <TextInput
              label={t('ai.modelName')}
              value={form.model_name}
              onChange={(v) => {
                setName(v);
                setResult(null);
              }}
              placeholder="work-assistant"
              maxLength={100}
            />
            <p className="text-xs text-muted">{t('ai.urlHint')}</p>
            {result && (
              <output
                className={`text-sm ${result.ok ? 'text-success' : 'text-danger'}`}
                data-testid="ai-test-result"
              >
                {result.text}
              </output>
            )}
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="secondary"
                isDisabled={!form.model_url.trim() || busy !== null}
                isPending={busy === 'test'}
                onPress={test}
                data-testid="ai-test"
              >
                {t('ai.test')}
              </Button>
              <Button
                type="submit"
                isDisabled={!form.model_url.trim() || busy !== null}
                isPending={busy === 'save'}
                data-testid="ai-save"
              >
                {t('save')}
              </Button>
              {settings.source === 'settings' && (
                <Button
                  type="button"
                  variant="danger-soft"
                  isDisabled={busy !== null}
                  isPending={busy === 'remove'}
                  onPress={remove}
                  data-testid="ai-remove"
                >
                  {t('ai.remove')}
                </Button>
              )}
            </div>
            <p className="text-xs text-muted" data-testid="ai-source">
              {t(`ai.source.${settings.source}`)}
              {settings.source === 'settings' && settings.environment_url
                ? ` ${t('ai.environmentFallback')}`
                : ''}
            </p>
          </form>
        </Panel.Content>
      </Panel>
      <Panel>
        <Panel.Header>
          <Panel.Title>{t('ai.whatTitle')}</Panel.Title>
          <Panel.Description>{t('ai.whatHelp')}</Panel.Description>
        </Panel.Header>
        <Panel.Content className="flex flex-col gap-3 text-sm">
          <ul className="list-disc pl-5 text-muted">
            {(
              [
                'writing',
                'summaries',
                'related',
                'suggestions',
                'reports',
                'rules',
              ] as const
            ).map((k) => (
              <li key={k}>{t(`ai.features.${k}`)}</li>
            ))}
          </ul>
          <p className="text-muted">{t('ai.servers')}</p>
          <p className="text-muted">{t('ai.privacy')}</p>
          <p className="text-muted">{t('ai.mcp')}</p>
        </Panel.Content>
      </Panel>
    </div>
  );
}
