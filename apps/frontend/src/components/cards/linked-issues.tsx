'use client';

import { Button, toast } from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Plus, Sparkles, X } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  searchWorkspaceCards,
  useCreateCardLink,
  useDeleteCardLink,
  useListCardLinks,
} from '../../api/generated/cards/cards';
import { relatedCards } from '../../api/generated/ai/ai';
import type {
  CardLinkOut,
  CardOut,
  RelatedOut,
} from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { issueTitle, issueType } from '../../lib/issue';
import { routes } from '../../lib/routes';
import {
  IssueTypeIcon,
  Lozenge,
  PriorityIcon,
  stageKind,
} from '../jira/issue-ui';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { SimpleSelect } from '../ui/inputs';

/**
 * The relationships Jira offers, each as the stored `link_type` and the direction it is read in.
 * One table for both ways, so the wording and what is sent to the server cannot drift apart.
 */
const RELATION_TABLE = {
  blocks: { link_type: 'blocks', direction: 'outward' },
  is_blocked_by: { link_type: 'blocks', direction: 'inward' },
  duplicates: { link_type: 'duplicates', direction: 'outward' },
  is_duplicated_by: { link_type: 'duplicates', direction: 'inward' },
  relates: { link_type: 'relates', direction: 'outward' },
  clones: { link_type: 'clones', direction: 'outward' },
  is_cloned_by: { link_type: 'clones', direction: 'inward' },
} as const;
type Relation = keyof typeof RELATION_TABLE;
const RELATIONS = Object.keys(RELATION_TABLE) as Relation[];

/** What the server stored (`link_type` + `direction`) -> the wording of this end. */
const toRelation = (type: string, direction: string): Relation =>
  RELATIONS.find(
    (r) =>
      RELATION_TABLE[r].link_type === type &&
      RELATION_TABLE[r].direction ===
        (type === 'relates' ? 'outward' : direction),
  ) ?? 'relates';

const fromRelation = (r: Relation) => RELATION_TABLE[r];

/**
 * Jira's "Linked work items": the issues this one blocks, is blocked by, duplicates, clones or
 * relates to, grouped by relationship, and a one-line form to add another.
 *
 * The picker suggests similar issues first (the built-in analysis); a close match preselects "duplicates".
 */
export function LinkedIssues({
  cardId,
  canWrite,
  stages,
  listId,
}: {
  cardId: string;
  canWrite: boolean;
  /** Stages of the issue's own list; a linked issue from another list shows no status. */
  stages?: { id: string; name: string; is_completed: boolean }[];
  listId?: string;
}) {
  const t = useTranslations('issue');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const fetched = body(useListCardLinks(cardId).data);
  const links = useMemo(() => fetched ?? [], [fetched]);
  const create = useCreateCardLink();
  const remove = useDeleteCardLink();
  const [open, setOpen] = useState(false);
  const fail = (e: unknown) =>
    toast.danger(serverMessage(e) ?? te(errorKind(e)));

  const groups = useMemo(() => {
    const by = new Map<Relation, CardLinkOut[]>();
    for (const l of links) {
      const r = toRelation(l.link_type, l.direction);
      by.set(r, [...(by.get(r) ?? []), l]);
    }
    return RELATIONS.filter((r) => by.has(r)).map((r) => ({
      relation: r,
      items: by.get(r) ?? [],
    }));
  }, [links]);

  return (
    <section aria-label={t('linkedIssues')} data-testid="linked-issues">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-base font-semibold">{t('linkedIssues')}</h3>
      </div>
      {groups.map((g) => (
        <div
          key={g.relation}
          className="mb-3"
          data-testid={`links-${g.relation}`}
        >
          <p className="jira-eyebrow mb-1">{t(`relation.${g.relation}`)}</p>
          <ul className="divide-y divide-separator overflow-hidden rounded-xl border border-separator">
            {g.items.map((l) => (
              <LinkRow
                key={l.id}
                link={l}
                stages={
                  l.card.lists[0]?.list_id === listId ? stages : undefined
                }
                canWrite={canWrite}
                onRemove={async () => {
                  try {
                    await remove.mutateAsync({ id: cardId, linkId: l.id });
                    await qc.invalidateQueries();
                  } catch (e) {
                    fail(e);
                  }
                }}
              />
            ))}
          </ul>
        </div>
      ))}
      {canWrite &&
        (open ? (
          <AddLink
            cardId={cardId}
            pending={create.isPending}
            onCancel={() => setOpen(false)}
            onAdd={async (relation, target) => {
              try {
                await create.mutateAsync({
                  id: cardId,
                  data: { ...fromRelation(relation), target_id: target },
                });
                await qc.invalidateQueries();
                setOpen(false);
              } catch (e) {
                fail(e);
              }
            }}
          />
        ) : (
          <button
            type="button"
            onClick={() => setOpen(true)}
            data-testid="add-link"
            className="flex items-center gap-1 rounded-sm px-1 py-1 text-sm text-muted hover:bg-default-hover hover:text-foreground"
          >
            <Plus size={14} />
            {t('addLink')}
          </button>
        ))}
      {!canWrite && links.length === 0 && (
        <p className="text-sm text-muted">{t('noLinks')}</p>
      )}
    </section>
  );
}

function LinkRow({
  link,
  stages,
  canWrite,
  onRemove,
}: {
  link: CardLinkOut;
  stages?: { id: string; name: string; is_completed: boolean }[];
  canWrite: boolean;
  onRemove: () => void;
}) {
  const t = useTranslations('issue');
  const ctx = useWorkspaceCtx();
  const c = link.card;
  const type = issueType(c, ctx.cardTypes);
  const stage = stages?.find((s) => s.id === c.lists[0]?.list_stage_id);
  return (
    <li
      className="group flex min-h-11 items-center gap-3 px-3 py-2 text-sm hover:bg-surface-secondary"
      data-testid="link-row"
    >
      <IssueTypeIcon icon={type?.icon} color={type?.color} title={type?.name} />
      <span className="w-16 shrink-0 text-xs text-muted md:w-20">{c.key}</span>
      <Link
        className="min-w-0 flex-1 truncate hover:underline"
        href={routes.browse(c.key)}
      >
        {issueTitle(c, ctx.cardTypes)}
      </Link>
      <PriorityIcon value={c.data.priority} />
      {stage && stages && (
        <Lozenge kind={stageKind(stage, stages[0]?.id === stage.id)}>
          {stage.name}
        </Lozenge>
      )}
      {canWrite && (
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={t('removeLink', { key: c.key })}
          className="md:opacity-0 md:group-hover:opacity-100 md:focus-visible:opacity-100"
          onPress={onRemove}
        >
          <X size={14} />
        </Button>
      )}
    </li>
  );
}

/** Relationship, then the issue to link (searched by key or summary), then Link. */
function AddLink({
  cardId,
  pending,
  onCancel,
  onAdd,
}: {
  cardId: string;
  pending: boolean;
  onCancel: () => void;
  onAdd: (relation: Relation, targetId: string) => void;
}) {
  const t = useTranslations('issue');
  const ctx = useWorkspaceCtx();
  const [relation, setRelation] = useState<Relation>('relates');
  const [q, setQ] = useState('');
  const [results, setResults] = useState<CardOut[]>([]);
  const [picked, setPicked] = useState<{ id: string; label: string } | null>(
    null,
  );
  const [similar, setSimilar] = useState<RelatedOut[]>([]);
  const searchable = q.trim().length >= 1 && !picked;
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  // Similar issues first, like Jira's "Searching for similar work items". Without the assistant (or
  // in the demo) the call fails and the picker is just the search.
  useEffect(() => {
    let live = true;
    relatedCards(cardId)
      .then((r) => live && setSimilar(body(r) ?? []))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [cardId]);

  useEffect(() => {
    if (!searchable) return;
    const handle = setTimeout(async () => {
      try {
        const found =
          body(
            await searchWorkspaceCards(ctx.workspaceId, { keyword: q.trim() }),
          ) ?? [];
        setResults(found.filter((c) => c.id !== cardId).slice(0, 8));
      } catch {
        setResults([]);
      }
    }, 200);
    return () => clearTimeout(handle);
  }, [q, searchable, ctx.workspaceId, cardId]);

  return (
    <div className="flex flex-col gap-2" data-testid="add-link-form">
      <div className="grid gap-2 sm:grid-cols-[12rem_minmax(0,1fr)]">
        <SimpleSelect
          hideLabel
          label={t('relationship')}
          value={relation}
          onChange={(v) => v && setRelation(v as Relation)}
          options={RELATIONS.map((r) => ({
            value: r,
            label: t(`relation.${r}`),
          }))}
          testId="link-relation"
        />
        <div className="relative">
          <input
            ref={input}
            value={picked ? picked.label : q}
            onChange={(e) => {
              setPicked(null);
              setQ(e.target.value.slice(0, 200));
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') onCancel();
              if (e.key === 'Enter' && picked) onAdd(relation, picked.id);
            }}
            aria-label={t('searchIssue')}
            placeholder={t('searchIssue')}
            data-testid="link-search"
            className="h-10 w-full rounded-sm border border-field-border bg-surface px-3 text-sm outline-none focus:border-accent"
          />
          {searchable && results.length > 0 && (
            <ul
              className="absolute left-0 right-0 top-11 z-20 max-h-64 overflow-y-auto rounded-lg border border-border bg-overlay p-1 shadow-overlay"
              data-testid="link-results"
            >
              {results.map((c) => {
                const type = issueType(c, ctx.cardTypes);
                return (
                  <li key={c.id}>
                    <button
                      type="button"
                      onClick={() =>
                        setPicked({
                          id: c.id,
                          label: `${c.key} ${issueTitle(c, ctx.cardTypes)}`,
                        })
                      }
                      className="flex min-h-10 w-full items-center gap-2 rounded-md px-2 text-left text-sm hover:bg-default"
                    >
                      <IssueTypeIcon icon={type?.icon} color={type?.color} />
                      <span className="shrink-0 text-xs text-muted">
                        {c.key}
                      </span>
                      <span className="truncate">
                        {issueTitle(c, ctx.cardTypes)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
      {!picked && !q.trim() && similar.length > 0 && (
        <div data-testid="similar-issues">
          <p className="jira-eyebrow mb-1 flex items-center gap-1">
            <Sparkles size={12} aria-hidden />
            {t('similarIssues')}
          </p>
          <ul className="flex flex-col">
            {similar.slice(0, 4).map((r) => (
              <li key={r.card_id}>
                <button
                  type="button"
                  onClick={() => {
                    setPicked({ id: r.card_id, label: `${r.key} ${r.title}` });
                    if (r.relation === 'duplicate') setRelation('duplicates');
                  }}
                  className="flex min-h-9 w-full items-center gap-2 rounded-md px-2 text-left text-sm hover:bg-default"
                >
                  <span className="shrink-0 text-xs text-muted">{r.key}</span>
                  <span className="truncate">{r.title}</span>
                  {r.relation === 'duplicate' && (
                    <Lozenge kind="progress">{t('possibleDuplicate')}</Lozenge>
                  )}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="tertiary" onPress={onCancel}>
          {t('cancel')}
        </Button>
        <Button
          size="sm"
          isDisabled={!picked}
          isPending={pending}
          onPress={() => picked && onAdd(relation, picked.id)}
          data-testid="link-submit"
        >
          {t('link')}
        </Button>
      </div>
    </div>
  );
}
