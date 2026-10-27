'use client';

import { routes } from '../../lib/routes';
import { ListBox, SearchField } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import {
  getCardByKey,
  searchWorkspaceCards,
} from '../../api/generated/cards/cards';
import type { CardOut } from '../../api/generated/model';
import { body } from '../../lib/api';
import { IssueTypeIcon } from '../jira/issue-ui';
import { useWorkspaceCtx } from './workspace-context';

/** Top-bar issue search: type a summary, or an issue key such as WORK-12 and press Enter. */
export function SearchBox() {
  const t = useTranslations('nav');
  const router = useRouter();
  const ctx = useWorkspaceCtx();
  const [q, setQ] = useState('');
  const [results, setResults] = useState<CardOut[]>([]);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLElement>(null);

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
          {shown.length === 0 ? (
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
        </div>
      )}
    </search>
  );
}
