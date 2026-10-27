'use client';

import { Button } from '@heroui/react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useLocale, useTranslations } from 'next-intl';
import { useState } from 'react';
import { issueTitle } from '../../lib/issue';
import { useWorkspaceCtx } from '../shell/workspace-context';
import type { ViewProps } from './types';

const key = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

export function CalendarView({ fields, cards, onOpen }: ViewProps) {
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const locale = useLocale();
  const [month, setMonth] = useState(
    () => new Date(new Date().getFullYear(), new Date().getMonth(), 1),
  );
  const due = fields.find((f) => f.slug === 'due_at');
  if (!due) return <p className="text-muted">{t('needsDueField')}</p>;

  const first = new Date(month);
  const offset = (first.getDay() + 6) % 7; // week starts on Monday
  const start = new Date(first.getFullYear(), first.getMonth(), 1 - offset);
  const days = Array.from(
    { length: 42 },
    (_, i) =>
      new Date(start.getFullYear(), start.getMonth(), start.getDate() + i),
  );
  const byDay = new Map<string, typeof cards>();
  for (const c of cards) {
    const v = c.data[due.slug];
    if (typeof v !== 'string') continue;
    const k = key(new Date(v.length === 10 ? `${v}T00:00:00` : v));
    byDay.set(k, [...(byDay.get(k) ?? []), c]);
  }
  const weekdays = Array.from({ length: 7 }, (_, i) =>
    new Intl.DateTimeFormat(locale, { weekday: 'short' }).format(
      new Date(2024, 0, 1 + i),
    ),
  );
  const [today] = useState(() => key(new Date()));

  return (
    <div data-testid="calendar">
      <div className="mb-2 flex items-center gap-2">
        <Button
          isIconOnly
          variant="ghost"
          aria-label={t('prevMonth')}
          onPress={() =>
            setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))
          }
        >
          <ChevronLeft size={16} />
        </Button>
        <h3 className="min-w-40 text-center font-medium">
          {new Intl.DateTimeFormat(locale, {
            month: 'long',
            year: 'numeric',
          }).format(month)}
        </h3>
        <Button
          isIconOnly
          variant="ghost"
          aria-label={t('nextMonth')}
          onPress={() =>
            setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))
          }
        >
          <ChevronRight size={16} />
        </Button>
      </div>
      <div className="grid grid-cols-7 gap-px overflow-hidden rounded-lg border border-separator bg-separator text-sm">
        {weekdays.map((w) => (
          <div
            key={w}
            className="bg-surface-secondary px-2 py-1 text-xs font-medium text-muted"
          >
            {w}
          </div>
        ))}
        {days.map((d) => {
          const items = byDay.get(key(d)) ?? [];
          return (
            <div
              key={key(d)}
              className={`min-h-24 bg-surface p-1 ${d.getMonth() === month.getMonth() ? '' : 'opacity-50'}`}
            >
              <div
                className={`mb-1 text-xs ${key(d) === today ? 'font-bold text-accent' : 'text-muted'}`}
              >
                {d.getDate()}
              </div>
              {items.slice(0, 3).map((c) => (
                <Button
                  key={c.id}
                  onPress={() => onOpen(c.id)}
                  variant="ghost"
                  size="sm"
                  className="mb-0.5 h-auto w-full justify-start rounded bg-accent-soft px-1 py-0 text-xs font-normal text-accent-soft-foreground"
                >
                  <span className="truncate">
                    {`${c.key} ${issueTitle(c, ctx.cardTypes)}`}
                  </span>
                </Button>
              ))}
              {items.length > 3 && (
                <div className="text-xs text-muted">+{items.length - 3}</div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
