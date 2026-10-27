'use client';

import { plainText, toDoc } from '../../lib/rich';
import { Chip } from '@heroui/react';
import { useLocale } from 'next-intl';
import type { FieldOut } from '../../api/generated/model';
import { asStrings } from '../../lib/cards';
import { formatDate, fullName } from '../../lib/format';

type Member = { user_id: string; first_name: string; last_name: string };

/** Read-only rendering of a card field value. Everything is rendered as text (React escapes it). */
export function FieldValue({
  field,
  value,
  members,
}: {
  field: FieldOut;
  value: unknown;
  members: Member[];
}) {
  const locale = useLocale();
  if (value === undefined || value === null || value === '')
    return <span className="text-muted">—</span>;
  switch (field.type) {
    case 'checkbox':
      return <span>{value ? '✓' : '—'}</span>;
    case 'date':
      return <span>{formatDate(value, locale)}</span>;
    case 'date_time':
      return <span>{formatDate(value, locale, true)}</span>;
    case 'number':
    case 'currency':
      return (
        <span>
          {typeof value === 'number'
            ? new Intl.NumberFormat(locale).format(value)
            : String(value)}
        </span>
      );
    case 'percentage':
      return <span>{String(value)}%</span>;
    case 'dropdown':
    case 'label':
      return (
        <span className="flex flex-wrap gap-1">
          {asStrings(value).map((v) => (
            <Chip key={v} size="sm">
              <Chip.Label>{v}</Chip.Label>
            </Chip>
          ))}
        </span>
      );
    case 'user':
      return (
        <span className="flex flex-wrap gap-1">
          {asStrings(value).map((id) => {
            const m = members.find((x) => x.user_id === id);
            return (
              <Chip key={id} size="sm">
                <Chip.Label>{m ? fullName(m) : '?'}</Chip.Label>
              </Chip>
            );
          })}
        </span>
      );
    case 'url':
      return typeof value === 'string' && /^https?:\/\//.test(value) ? (
        <a
          href={value}
          target="_blank"
          rel="noopener noreferrer nofollow"
          className="text-accent underline"
        >
          {value}
        </a>
      ) : (
        <span>{String(value)}</span>
      );
    case 'rich':
      return <span className="line-clamp-2">{plainText(toDoc(value))}</span>;
    default:
      return <span>{String(value)}</span>;
  }
}
