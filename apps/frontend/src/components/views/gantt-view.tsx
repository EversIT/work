'use client';

import { Button } from '@heroui/react';
import { useLocale, useTranslations } from 'next-intl';
import { issueTitle } from '../../lib/issue';
import { useWorkspaceCtx } from '../shell/workspace-context';
import type { ViewProps } from './types';

const DAY = 86_400_000;
const parse = (v: unknown) =>
  typeof v === 'string' && v
    ? new Date(v.length === 10 ? `${v}T00:00:00` : v).getTime()
    : NaN;

/** Timeline of cards that have a start and/or due date. Cards without any date are listed below. */
export function GanttView({ fields, cards, onOpen }: ViewProps) {
  const t = useTranslations('views');
  const ctx = useWorkspaceCtx();
  const locale = useLocale();
  const starts = fields.find((f) => f.slug === 'starts_at');
  const due = fields.find((f) => f.slug === 'due_at');
  if (!due && !starts)
    return <p className="text-muted">{t('needsDateFields')}</p>;

  const rows = cards.map((c) => {
    const s = parse(starts && c.data[starts.slug]);
    const d = parse(due && c.data[due.slug]);
    const from = Number.isNaN(s) ? d : s;
    const to = Number.isNaN(d) ? s : d;
    return { card: c, from, to };
  });
  const dated = rows.filter(
    (r) => !Number.isNaN(r.from) && !Number.isNaN(r.to),
  );
  const undated = rows.filter(
    (r) => Number.isNaN(r.from) || Number.isNaN(r.to),
  );
  if (dated.length === 0)
    return <p className="text-muted">{t('noDatedCards')}</p>;

  const min = Math.min(...dated.map((r) => Math.min(r.from, r.to))) - 2 * DAY;
  const max = Math.max(...dated.map((r) => Math.max(r.from, r.to))) + 3 * DAY;
  const total = Math.max(max - min, 7 * DAY);
  const ticks = Array.from(
    { length: Math.min(12, Math.ceil(total / (7 * DAY))) + 1 },
    (_, i) => min + i * (total / Math.min(12, Math.ceil(total / (7 * DAY)))),
  );
  const fmt = new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'short',
  });
  const pct = (x: number) => `${((x - min) / total) * 100}%`;

  return (
    <div data-testid="gantt" className="overflow-x-auto">
      <div className="min-w-[640px]">
        <div className="relative ml-48 h-6 border-b border-separator text-xs text-muted">
          {ticks.map((tk) => (
            <span
              key={tk}
              className="absolute -translate-x-1/2"
              style={{ left: pct(tk) }}
            >
              {fmt.format(tk)}
            </span>
          ))}
        </div>
        {dated.map(({ card, from, to }) => (
          <div
            key={card.id}
            className="flex items-center border-b border-separator/50 py-1"
          >
            <Button
              onPress={() => onOpen(card.id)}
              variant="ghost"
              size="sm"
              className="w-48 shrink-0 justify-start px-1 font-normal"
            >
              <span className="truncate">
                {`${card.key} ${issueTitle(card, ctx.cardTypes)}`}
              </span>
            </Button>
            <div className="relative h-6 flex-1">
              <div
                className="absolute top-1 h-4 rounded bg-accent"
                style={{
                  left: pct(Math.min(from, to)),
                  width: `max(${(Math.abs(to - from) / total) * 100}%, 0.75rem)`,
                }}
                title={`${fmt.format(Math.min(from, to))} – ${fmt.format(Math.max(from, to))}`}
              />
            </div>
          </div>
        ))}
        {undated.length > 0 && (
          <p className="mt-3 text-xs text-muted">
            {t('undated', { count: undated.length })}:{' '}
            {undated
              .map((r) => `${r.card.key} ${issueTitle(r.card, ctx.cardTypes)}`)
              .join(', ')}
          </p>
        )}
      </div>
    </div>
  );
}
