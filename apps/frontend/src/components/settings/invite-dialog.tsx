'use client';

import { Button, toast } from '@heroui/react';
import { Copy } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import { errorKind, serverMessage } from '../../lib/api';
import { formatDate } from '../../lib/format';
import { BASE_PATH } from '../../lib/routes';
import { FormDialog } from '../ui/form-dialog';
import { ReadonlyField, SimpleSelect, TextInput } from '../ui/inputs';

/**
 * Invite people with a one-time link or by e-mail. The same dialog serves the workspace and a
 * single project; the caller says what an invitation does by passing the two request functions.
 */
export function InviteDialog({
  isOpen,
  onClose,
  roles,
  mailEnabled,
  onLink,
  onEmail,
}: {
  isOpen: boolean;
  onClose: () => void;
  roles: string[];
  mailEnabled: boolean;
  onLink: (o: {
    role: string;
    hours: number;
    uses: number;
  }) => Promise<{ token: string; expires_at: string }>;
  onEmail: (o: { email: string; role: string }) => Promise<void>;
}) {
  const t = useTranslations('project');
  const te = useTranslations('errors');
  const locale = useLocale();
  const [role, setRole] = useState<string | null>(
    roles.includes('member') ? 'member' : (roles[0] ?? null),
  );
  const [link, setLink] = useState<{ url: string; expires: string } | null>(
    null,
  );
  const [mailInvited, setMailInvited] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const close = () => {
    setLink(null);
    setMailInvited(false);
    setError(null);
    onClose();
  };

  return (
    <FormDialog
      isOpen={isOpen}
      onClose={close}
      title={t('invite')}
      submitLabel={link || mailInvited ? t('close') : t('createInvite')}
      error={error}
      isPending={pending}
      onSubmit={async (f) => {
        if (link || mailInvited) return close();
        setError(null);
        setPending(true);
        try {
          const email = String(f.get('email') ?? '').trim();
          if (email) {
            await onEmail({ email, role: role ?? 'member' });
            setMailInvited(true);
            return;
          }
          const res = await onLink({
            role: role ?? 'member',
            hours: Number(f.get('hours') ?? 72),
            uses: Number(f.get('uses') ?? 1),
          });
          setLink({
            url: `${window.location.origin}${BASE_PATH}/invite?token=${encodeURIComponent(res.token)}`,
            expires: res.expires_at,
          });
        } catch (e) {
          setError(serverMessage(e) ?? te(errorKind(e)));
        } finally {
          setPending(false);
        }
      }}
    >
      {link ? (
        <div className="flex flex-col gap-2">
          <p className="text-sm">
            {t('inviteReady', { date: formatDate(link.expires, locale, true) })}
          </p>
          <div className="flex items-center gap-2">
            <ReadonlyField
              label={t('inviteLink')}
              value={link.url}
              testId="invite-link"
            />
            <Button
              isIconOnly
              variant="tertiary"
              aria-label={t('copy')}
              onPress={() =>
                navigator.clipboard
                  ?.writeText(link.url)
                  .then(() => toast.success(t('copied')))
              }
            >
              <Copy size={14} />
            </Button>
          </div>
          <p className="text-xs text-muted">{t('inviteOnce')}</p>
        </div>
      ) : mailInvited ? (
        <p className="text-sm" data-testid="mail-invited">
          {t('mailSent')}
        </p>
      ) : (
        <>
          <SimpleSelect
            label={t('role')}
            value={role}
            onChange={setRole}
            options={roles.map((r) => ({ value: r, label: t(`roles.${r}`) }))}
            testId="invite-role"
          />
          <TextInput
            label={t('expiresInHours')}
            name="hours"
            type="number"
            defaultValue="72"
            required
          />
          <TextInput
            label={t('maxUses')}
            name="uses"
            type="number"
            defaultValue="1"
            required
          />
          {mailEnabled && (
            <div className="rounded-lg bg-surface-secondary p-3">
              <TextInput
                label={t('inviteEmail')}
                name="email"
                type="email"
                placeholder={t('inviteEmailHint')}
              />
              <p className="mt-1 text-xs text-muted">
                {t('inviteEmailExplain')}
              </p>
            </div>
          )}
        </>
      )}
    </FormDialog>
  );
}
