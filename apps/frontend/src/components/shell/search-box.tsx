'use client';

import { routes } from '../../lib/routes';
import { ListBox, SearchField, Spinner } from '@heroui/react';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import {
  getCardByKey,
  searchCards,
  searchWorkspaceCards,
} from '../../api/generated/cards/cards';
import { getList } from '../../api/generated/lists/lists';
import type { CardOut, FieldOut } from '../../api/generated/model';
import { body } from '../../lib/api';
import { parseFilterText, type Part } from '../../lib/filter-text';
import { rowsToFilter } from '../views/filter-builder';
import { IssueTypeIcon } from '../jira/issue-ui';
import { useWorkspaceCtx } from './workspace-context';
import { useAiStatus } from '../../lib/ai';

/** Top-bar issue search: type a summary, or an issue key such as WORK-12 and press Enter. */
export function SearchBox() {
  const t = useTranslations('nav');
  const router = useRouter();
  const ctx = useWorkspaceCtx();
  const [q, setQ] = useState('');
  const [results, setResults] = useState<CardOut[]>([]);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLElement>(null);
  const ai = useAiStatus();
  const [asking, setAsking] = useState(false);
  const [answer, setAnswer] = useState<{
    parts: Part[];
    cards: CardOut[];
  } | null>(null);

  // Short queries show nothing; the stored results only matter once a search was sent.
  const searchable = q.trim().length >= 2;
  const shown = searchable ? results : [];
  useEffect(() => {
    if (!searchable) return;
    const handle = setTimeout(async () => {
      try {
        setResults(
          body(
            await searchWorkspaceCards(ctx.workspaceId, { keyword: q.trim() }),
          ) ?? [],
        );
      } catch {
        setResults([]);
      }
    }, 200);
    return () => clearTimeout(handle);
  }, [q, searchable, ctx.workspaceId]);

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, []);

  /**
   * "Ask": a sentence instead of a keyword ("bugs assigned to me, high priority, created last week") is
   * turned into the same conditions the filter builder uses and run in every list of the workspace. The
   * conditions are shown back, so it is clear what was asked.
   */
  const ask = async () => {
    setAsking(true);
    try {
      const lists = ctx.lists.slice(0, 12);
      const details = await Promise.all(
        lists.map((l) =>
          getList(l.id)
            .then(body)
            .catch(() => undefined),
        ),
      );
      const fields = [
        ...new Map(
          ctx.cardTypes.flatMap((c) => c.fields).map((f) => [f.slug, f]),
        ).values(),
      ] as FieldOut[];
      let parts: Part[] = [];
      const found = await Promise.all(
        lists.map(async (l, i) => {
          const d = details[i];
          if (!d) return [];
          const parsed = parseFilterText(q, {
            fields,
            stages: d.stages,
            members: ctx.members,
            cardTypes: ctx.cardTypes,
            meId: ctx.me?.id,
          });
          if (parsed.parts.length > parts.length) parts = parsed.parts;
          const filters = rowsToFilter(parsed.rows, fields);
          if (!filters) return [];
          const page = body(
            await searchCards(l.id, {
              filters,
              limit: 10,
              sort_by: 'updated_at',
              sort_order: 'desc',
            }),
          );
          return page?.cards ?? [];
        }),
      );
      const seen = new Set<string>();
      const cards = found
        .flat()
        .filter((c) => !seen.has(c.id) && !!seen.add(c.id))
        .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
        .slice(0, 15);
      setAnswer({ parts, cards });
    } catch {
      setAnswer({ parts: [], cards: [] });
    } finally {
      setAsking(false);
    }
  };
  const partText = (p: Part) => {
    const value = /^\d+d$/.test(p.value)
      ? t('askDays', { n: p.value.slice(0, -1) })
      : [
            'me',
            'unassigned',
            'open',
            'today',
            'yesterday',
            'thisWeek',
            'thisMonth',
          ].includes(p.value)
        ? t(`askValue.${p.value}`)
        : p.value;
    const label = p.kind === 'field' ? (p.field ?? '') : t(`askKind.${p.kind}`);
    return label ? `${label}: ${value}` : value;
  };

  const go = (issueKey: string) => {
    setOpen(false);
    setQ('');
    router.push(routes.browse(issueKey));
  };
  const titleSlug = (c: CardOut) => {
    const type = ctx.cardTypes.find((x) => x.id === c.card_type_id);
    const f = type?.fields.find((x) => x.is_title);
    return f ? String(c.data[f.slug] ?? '') : '';
  };

  return (
    <search ref={box} className="relative block w-full max-w-xs">
      <SearchField
        fullWidth
        aria-label={t('search')}
        value={q}
        onChange={(v) => {
          setQ(v.slice(0, 200));
          setAnswer(null);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onClear={() => setQ('')}
        onKeyDown={async (e) => {
          if (e.key === 'Escape') setOpen(false);
          if (e.key === 'Enter' && q.trim()) {
            // An exact key jumps straight to the issue.
            if (/^[A-Za-z][A-Za-z0-9]+-\d+$/.test(q.trim())) {
              try {
                const c = body(await getCardByKey(ctx.workspaceId, q.trim()));
                if (c) return go(c.key);
              } catch {
                /* fall through to the result list */
              }
            }
            if (shown[0]) go(shown[0].key);
          }
        }}
      >
        <SearchField.Group>
          <SearchField.SearchIcon />
          <SearchField.Input
            placeholder={t('search')}
            data-testid="top-search"
          />
          <SearchField.ClearButton />
        </SearchField.Group>
      </SearchField>
      {open && q.trim().length >= 2 && (
        <div
          className="absolute left-0 right-0 top-10 z-50 max-h-80 overflow-y-auto rounded-lg border border-border bg-overlay p-1 shadow-overlay"
          data-testid="top-search-results"
        >
          {answer ? (
            <div data-testid="ask-answer">
              {answer.parts.length > 0 ? (
                <p className="px-3 py-2 text-xs text-muted">
                  {t('askUnderstood')}: {answer.parts.map(partText).join(' · ')}
                </p>
              ) : (
                <p className="px-3 py-2 text-sm text-muted">
                  {t('askNothing')}
                </p>
              )}
              {answer.cards.length === 0 ? (
                answer.parts.length > 0 && (
                  <p className="px-3 py-2 text-sm text-muted">
                    {t('noResults')}
                  </p>
                )
              ) : (
                <ListBox
                  aria-label={t('askResults')}
                  onAction={(key) => {
                    const hit = answer.cards.find((x) => x.id === String(key));
                    if (hit) go(hit.key);
                  }}
                >
                  {answer.cards.map((c) => {
                    const type = ctx.cardTypes.find(
                      (x) => x.id === c.card_type_id,
                    );
                    return (
                      <ListBox.Item key={c.id} id={c.id} textValue={c.key}>
                        <IssueTypeIcon icon={type?.icon} color={type?.color} />
                        <span className="text-xs text-muted">{c.key}</span>
                        <span className="truncate">{titleSlug(c)}</span>
                      </ListBox.Item>
                    );
                  })}
                </ListBox>
              )}
            </div>
          ) : shown.length === 0 ? (
            <p className="px-3 py-2 text-sm text-muted">{t('noResults')}</p>
          ) : (
            <ListBox
              aria-label={t('search')}
              onAction={(key) => {
                const hit = shown.find((x) => x.id === String(key));
                if (hit) go(hit.key);
              }}
            >
              {shown.map((c) => {
                const type = ctx.cardTypes.find((x) => x.id === c.card_type_id);
                return (
                  <ListBox.Item key={c.id} id={c.id} textValue={c.key}>
                    <IssueTypeIcon icon={type?.icon} color={type?.color} />
                    <span className="text-xs text-muted">{c.key}</span>
                    <span className="truncate">{titleSlug(c)}</span>
                  </ListBox.Item>
                );
              })}
            </ListBox>
          )}
          {ai && !answer && q.trim().split(/\s+/).length >= 2 && (
            <button
              type="button"
              onClick={ask}
              disabled={asking}
              data-testid="ask-ai"
              className="mt-1 flex w-full items-center gap-2 rounded-md border-t border-separator px-3 py-2 text-left text-sm text-accent hover:bg-default"
            >
              {asking ? (
                <Spinner size="sm" />
              ) : (
                <Sparkles size={14} aria-hidden />
              )}
              {t('askRow', { q: q.trim() })}
            </button>
          )}
        </div>
      )}
    </search>
  );
}
