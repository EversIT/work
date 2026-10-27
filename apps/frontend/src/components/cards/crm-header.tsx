'use client';

import { Building2, Mail, Phone, Globe } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import type { CardOut, CardTypeOut } from '../../api/generated/model';
import { formatDate, initials } from '../../lib/format';

/** Only plain web links are made clickable: a `javascript:` URL typed into a field must stay text. */
export function safeWebUrl(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  try {
    const u = new URL(value.includes('://') ? value : `https://${value}`);
    return u.protocol === 'https:' || u.protocol === 'http:'
      ? u.toString()
      : null;
  } catch {
    return null;
  }
}

const str = (v: unknown) =>
  typeof v === 'string' && v.trim() ? v.trim() : null;
const num = (v: unknown) =>
  typeof v === 'number' && Number.isFinite(v) ? v : null;

/**
 * The summary block of the CRM layouts (contact, company, deal) above the issue's own fields.
 * It reads fields by their well-known keys (`email`, `amount`, ...) that the layout seeds.
 */
export function CrmHeader({
  card,
  type,
  title,
}: {
  card: CardOut;
  type: CardTypeOut;
  title: string;
}) {
  const t = useTranslations('crm');
  const locale = useLocale();
  const d = card.data;
  // Read once per mount: rendering must not depend on the clock.
  const [now] = useState(() => Date.now());
  if (type.layout === 'person') {
    const [first, ...rest] = title.split(' ');
    const sub = [str(d.job_title), str(d.company)].filter(Boolean).join(' · ');
    const email = str(d.email);
    const phone = str(d.phone);
    return (
      <section
        className="flex items-center gap-4 rounded-sm border border-border bg-surface-secondary p-4"
        data-testid="crm-person"
        aria-label={t('person')}
      >
        <span
          className="flex size-14 shrink-0 items-center justify-center rounded-full bg-accent text-lg font-semibold text-white"
          aria-hidden
        >
          {initials({ first_name: first ?? '', last_name: rest.join(' ') })}
        </span>
        <div className="min-w-0">
          <p className="truncate text-lg font-semibold">{title}</p>
          {sub && <p className="truncate text-sm text-muted">{sub}</p>}
          <p className="mt-1 flex flex-wrap gap-4 text-sm">
            {email && (
              <a
                className="flex items-center gap-1 text-accent hover:underline"
                href={`mailto:${encodeURI(email)}`}
              >
                <Mail size={14} />
                {email}
              </a>
            )}
            {phone && (
              <a
                className="flex items-center gap-1 text-accent hover:underline"
                href={`tel:${phone.replace(/[^+\d]/g, '')}`}
              >
                <Phone size={14} />
                {phone}
              </a>
            )}
          </p>
        </div>
      </section>
    );
  }
  if (type.layout === 'organization') {
    const site = safeWebUrl(d.website);
    const employees = num(d.employees);
    return (
      <section
        className="flex items-center gap-4 rounded-sm border border-border bg-surface-secondary p-4"
        data-testid="crm-organization"
        aria-label={t('organization')}
      >
        <span
          className="flex size-14 shrink-0 items-center justify-center rounded-md bg-success text-white"
          aria-hidden
        >
          <Building2 size={26} />
        </span>
        <div className="min-w-0">
          <p className="truncate text-lg font-semibold">{title}</p>
          <p className="text-sm text-muted">
            {[
              str(d.industry),
              employees !== null ? t('employees', { count: employees }) : null,
            ]
              .filter(Boolean)
              .join(' · ')}
          </p>
          {site && (
            <a
              className="mt-1 flex items-center gap-1 text-sm text-accent hover:underline"
              href={site}
              target="_blank"
              rel="noopener noreferrer"
            >
              <Globe size={14} />
              {site.replace(/^https?:\/\//, '').replace(/\/$/, '')}
            </a>
          )}
        </div>
      </section>
    );
  }
  if (type.layout === 'deal') {
    const amount = num(d.amount);
    const probability = num(d.probability);
    const close = str(d.expected_close);
    const days = close
      ? Math.ceil((new Date(close).getTime() - now) / 86_400_000)
      : null;
    return (
      <section
        className="flex flex-wrap items-end justify-between gap-4 rounded-sm border border-border bg-surface-secondary p-4"
        data-testid="crm-deal"
        aria-label={t('deal')}
      >
        <div>
          <p className="text-xs uppercase text-muted">{t('amount')}</p>
          <p className="text-2xl font-semibold" data-testid="deal-amount">
            {amount !== null
              ? new Intl.NumberFormat(locale, {
                  minimumFractionDigits: 2,
                  maximumFractionDigits: 2,
                }).format(amount)
              : '—'}
          </p>
        </div>
        {probability !== null && (
          <div className="w-40">
            <p className="text-xs uppercase text-muted">
              {t('probability')}: {probability}%
            </p>
            <progress
              className="mt-1 h-2 w-full overflow-hidden rounded [&::-moz-progress-bar]:bg-success [&::-webkit-progress-bar]:bg-border [&::-webkit-progress-value]:bg-success"
              value={Math.min(100, Math.max(0, probability))}
              max={100}
            />
          </div>
        )}
        {close && (
          <div>
            <p className="text-xs uppercase text-muted">{t('expectedClose')}</p>
            <p className="text-sm">
              {formatDate(close, locale)}
              {days !== null && (
                <span className="ml-2 text-xs text-muted">
                  {days >= 0
                    ? t('inDays', { count: days })
                    : t('daysAgo', { count: -days })}
                </span>
              )}
            </p>
          </div>
        )}
      </section>
    );
  }
  return null;
}
