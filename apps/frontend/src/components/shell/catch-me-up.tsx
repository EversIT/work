'use client';

import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useState } from 'react';
import { useCatchUp } from '../../api/generated/ai/ai';
import { body } from '../../lib/api';
import { routes } from '../../lib/routes';
import { sinceLastVisit } from '../../lib/visit';
import { useAiStatus } from '../../lib/ai';

function Issues({
  items,
}: {
  items: { card_id: string; key: string; title: string }[];
}) {
  return (
    <>
      {items.slice(0, 4).map((c, i) => (
        <span key={c.card_id}>
          {i > 0 && ', '}
          <Link
            href={routes.browse(c.key)}
            title={c.title}
            className="font-medium text-accent hover:underline"
          >
            {c.key}
          </Link>
        </span>
      ))}
    </>
  );
}

/**
 * "For you": a few sentences about what happened since the last visit, with links. Counted from the issues
 * (your open work, what got finished, new comments, mentions), not written by a model.
 */
function CatchMeUpPanel({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations('workspace');
  // The visit bookkeeping lives in the browser; on the server (and without storage) it is a week.
  const [since] = useState(() => {
    let store: Storage | null = null;
    try {
      store = typeof window === 'undefined' ? null : window.localStorage;
    } catch {
      store = null;
    }
    return sinceLastVisit(store, workspaceId).toISOString();
  });
  const query = useCatchUp(workspaceId, { since }, { query: { retry: false } });
  const d = body(query.data);
  if (query.isLoading)
    return (
      <div
        className="mt-3 h-5 w-2/3 animate-pulse rounded bg-default"
        aria-hidden
        data-testid="catch-skeleton"
      />
    );
  if (!d) return null; // the assistant is not there (demo, old server): nothing to show
  const quiet =
    d.assigned_open === 0 &&
    d.finished.length === 0 &&
    d.commented.length === 0 &&
    d.mentioned.length === 0;
  return (
    <div
      className="mt-3 flex items-start gap-2 text-sm"
      data-testid="catch-me-up"
    >
      <Sparkles size={14} aria-hidden className="mt-0.5 shrink-0 text-accent" />
      <div className="flex flex-col gap-0.5">
        {quiet && <p className="text-muted">{t('catchNothing')}</p>}
        {d.mentioned.length > 0 && (
          <p>
            {t('catchMentioned')} <Issues items={d.mentioned} />.
          </p>
        )}
        {d.commented.length > 0 && (
          <p>
            {t('catchCommented', {
              count: d.commented.reduce((n, c) => n + c.count, 0),
            })}{' '}
            <Issues items={d.commented} />.
          </p>
        )}
        {d.finished.length > 0 && (
          <p>
            {t('catchFinished', { count: d.finished.length })}{' '}
            <Issues items={d.finished} />.
          </p>
        )}
        {d.assigned_open > 0 && (
          <p>{t('catchAssigned', { count: d.assigned_open })}</p>
        )}
      </div>
    </div>
  );
}

export function CatchMeUp(props: { workspaceId: string }) {
  return useAiStatus() ? <CatchMeUpPanel {...props} /> : null;
}
