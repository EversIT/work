'use client';

import { Button, Card, Chip, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Copy, Trash2 } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import { useCreateApiToken, useListApiTokens, useRevokeApiToken } from '../../api/generated/api-tokens/api-tokens';
import { body, errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { FormDialog } from '../ui/form-dialog';
import { SimpleSelect, TextInput } from '../ui/inputs';

/** Personal API tokens, like Jira's: shown once, scoped to read or write, revocable. */
export function ApiTokenSettings() {
  const t = useTranslations('apiTokens');
  const te = useTranslations('errors');
  const locale = useLocale();
  const qc = useQueryClient();
  const tokens = body(useListApiTokens().data) ?? [];
  const create = useCreateApiToken();
  const revoke = useRevokeApiToken();
  const [open, setOpen] = useState(false);
  const [scope, setScope] = useState<string | null>('write');
  const [expiry, setExpiry] = useState<string | null>('90');
  const [created, setCreated] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const origin = typeof window === 'undefined' ? '' : window.location.origin;

  return (
    <Card data-testid="api-tokens">
      <Card.Header className="flex-row items-center justify-between">
        <div>
          <Card.Title>{t('title')}</Card.Title>
          <Card.Description>{t('description')}</Card.Description>
        </div>
        <Button size="sm" onPress={() => { setCreated(null); setError(null); setOpen(true); }} data-testid="create-token">{t('create')}</Button>
      </Card.Header>
      <Card.Content className="flex flex-col gap-3">
        {tokens.length === 0 && <p className="text-sm text-muted">{t('none')}</p>}
        <ul className="divide-y divide-[#DCDFE4]" data-testid="token-list">
          {tokens.map((tk) => (
            <li key={tk.id} className="flex items-center gap-3 py-2 text-sm">
              <div className="min-w-0 flex-1">
                <p className="font-medium">{tk.name}</p>
                <p className="text-xs text-muted">
                  <code>{tk.token_prefix}…</code> · {t('lastUsed')}: {tk.last_used_at ? formatDate(tk.last_used_at, locale, true) : t('never')}
                  {tk.expires_at ? ` · ${t('expires')} ${formatDate(tk.expires_at, locale)}` : ''}
                </p>
              </div>
              <Chip size="sm"><Chip.Label>{t(`scope.${tk.scope}`)}</Chip.Label></Chip>
              <Button isIconOnly size="sm" variant="ghost" aria-label={t('revoke')}
                onPress={async () => {
                  if (!window.confirm(t('confirmRevoke'))) return;
                  try { await revoke.mutateAsync({ id: tk.id }); await qc.invalidateQueries(); } catch (e) { toast.danger(serverMessage(e) ?? te(errorKind(e))); }
                }}>
                <Trash2 size={14} />
              </Button>
            </li>
          ))}
        </ul>
        <div className="rounded-[3px] bg-[#F7F8F9] p-3 text-xs">
          <p className="mb-1 font-semibold">{t('usage')}</p>
          <pre className="overflow-x-auto whitespace-pre-wrap break-all">{`curl -H "Authorization: Bearer wrk_…" ${origin}/api/v1/me`}</pre>
          <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-all">{`curl -u you@example.com:wrk_… ${origin}/api/v1/projects`}</pre>
          <a className="mt-1 inline-block text-[#0C66E4] underline" href="/api/v1/openapi.json" target="_blank" rel="noopener noreferrer">{t('openapi')}</a>
        </div>
      </Card.Content>

      {open && (
        <FormDialog
          isOpen
          onClose={() => setOpen(false)}
          title={created ? t('copyTitle') : t('create')}
          submitLabel={created ? t('done') : t('createToken')}
          isPending={create.isPending}
          error={error}
          onSubmit={async (f) => {
            if (created) return setOpen(false);
            setError(null);
            try {
              const res = await create.mutateAsync({ data: { name: String(f.get('name')), scope: scope ?? 'write', expires_in_days: expiry && expiry !== 'never' ? Number(expiry) : null } });
              if (res.status === 200) setCreated(res.data.token);
              await qc.invalidateQueries();
            } catch (e) {
              setError(serverMessage(e) ?? te(errorKind(e)));
            }
          }}
        >
          {created ? (
            <div className="flex flex-col gap-2">
              <p className="text-sm">{t('copyHint')}</p>
              <div className="flex items-center gap-2">
                <input readOnly aria-label={t('token')} value={created} data-testid="new-token" onFocus={(e) => e.currentTarget.select()} className="h-9 flex-1 rounded-[3px] border border-[#8590A2] bg-white px-2 font-mono text-xs" />
                <Button isIconOnly variant="tertiary" aria-label={t('copy')} onPress={() => navigator.clipboard?.writeText(created).then(() => toast.success(t('copied')))}><Copy size={14} /></Button>
              </div>
            </div>
          ) : (
            <>
              <TextInput label={t('name')} name="name" required maxLength={100} autoFocus placeholder={t('namePlaceholder')} />
              <SimpleSelect label={t('permissions')} value={scope} onChange={setScope} options={[{ value: 'write', label: t('scope.write') }, { value: 'read', label: t('scope.read') }]} testId="token-scope" />
              <SimpleSelect label={t('expiry')} value={expiry} onChange={setExpiry} options={[{ value: '30', label: t('days', { count: 30 }) }, { value: '90', label: t('days', { count: 90 }) }, { value: '365', label: t('days', { count: 365 }) }, { value: 'never', label: t('neverExpires') }]} />
            </>
          )}
        </FormDialog>
      )}
    </Card>
  );
}
