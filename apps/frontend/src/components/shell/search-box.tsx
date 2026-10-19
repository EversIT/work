'use client';

import { Search } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { getCardByKey, searchWorkspaceCards } from '../../api/generated/cards/cards';
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
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (q.trim().length < 2) {
      setResults([]);
      return;
    }
    const handle = setTimeout(async () => {
      try {
        setResults(body(await searchWorkspaceCards(ctx.workspaceId, { keyword: q.trim() })) ?? []);
      } catch {
        setResults([]);
      }
    }, 200);
    return () => clearTimeout(handle);
  }, [q, ctx.workspaceId]);

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, []);

  const go = (id: string) => {
    setOpen(false);
    setQ('');
    router.push(`/workspaces/${ctx.workspaceId}/cards/${id}`);
  };
  const titleSlug = (c: CardOut) => {
    const type = ctx.cardTypes.find((x) => x.id === c.card_type_id);
    const f = type?.fields.find((x) => x.is_title);
    return f ? String(c.data[f.slug] ?? '') : '';
  };

  return (
    <div ref={box} className="relative w-full max-w-xs" role="search">
      <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted" />
      <input
        aria-label={t('search')}
        placeholder={t('search')}
        value={q}
        maxLength={200}
        data-testid="top-search"
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={async (e) => {
          if (e.key === 'Escape') setOpen(false);
          if (e.key === 'Enter' && q.trim()) {
            // An exact key jumps straight to the issue.
            if (/^[A-Za-z][A-Za-z0-9]+-\d+$/.test(q.trim())) {
              try {
                const c = body(await getCardByKey(ctx.workspaceId, q.trim()));
                if (c) return go(c.id);
              } catch {
                /* fall through to the result list */
              }
            }
            if (results[0]) go(results[0].id);
          }
        }}
        className="h-8 w-full rounded-[3px] border border-[#8590A2] bg-white pl-8 pr-2 text-sm placeholder:text-[#758195] focus:border-[#0C66E4] focus:outline-none focus:ring-1 focus:ring-[#0C66E4]"
      />
      {open && q.trim().length >= 2 && (
        <ul className="absolute left-0 right-0 top-9 z-50 max-h-80 overflow-y-auto rounded-[3px] border border-[#DCDFE4] bg-white py-1 shadow-lg" data-testid="top-search-results">
          {results.length === 0 && <li className="px-3 py-2 text-sm text-muted">{t('noResults')}</li>}
          {results.map((c) => {
            const type = ctx.cardTypes.find((x) => x.id === c.card_type_id);
            return (
              <li key={c.id}>
                <button className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-[#F1F2F4]" onClick={() => go(c.id)}>
                  <IssueTypeIcon icon={type?.icon} color={type?.color} />
                  <span className="text-xs text-muted">{c.key}</span>
                  <span className="truncate">{titleSlug(c)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
