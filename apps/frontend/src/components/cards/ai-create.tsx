'use client';

import { Button, Spinner, toast } from '@heroui/react';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import {
  draftDescription,
  similarIssues,
  suggestDraftFields,
} from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import type { FieldSuggestion, RelatedOut } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { routes } from '../../lib/routes';
import { Lozenge } from '../jira/issue-ui';

/** A title shorter than this says too little to look for similar issues. */
const MIN_TITLE = 8;

/**
 * Help while an issue is being written: similar issues (against duplicates), values for the empty fields
 * from similar issues, and a first draft of the description. Everything is a suggestion; the person
 * applies it, and nothing is submitted for them.
 */
export function AiCreateHelper({
  workspaceId,
  cardTypeId,
  kind,
  title,
  description,
  filled,
  descriptionSlug,
  onApply,
}: {
  workspaceId: string;
  cardTypeId?: string;
  /** Name of the issue type, for the layout of a drafted description. */
  kind?: string;
  title: string;
  /** The description as plain text. */
  description: string;
  /** Slugs of the fields that already have a value. */
  filled: string[];
  /** The rich-text description field, when the type has one. */
  descriptionSlug?: string;
  onApply: (slug: string, value: unknown) => void;
}) {
  const t = useTranslations('ai');
  const te = useTranslations('errors');
  const status = useAiStatus();
  const [similar, setSimilar] = useState<RelatedOut[]>([]);
  const [suggested, setSuggested] = useState<FieldSuggestion[] | null>(null);
  const [busy, setBusy] = useState<'fields' | 'draft' | null>(null);
  const ready = title.trim().length >= MIN_TITLE;

  // Look for similar issues while the summary is typed (and the description, once there is one).
  useEffect(() => {
    if (!status || !ready) return;
    let live = true;
    const handle = setTimeout(async () => {
      try {
        const r = await similarIssues({
          workspace_id: workspaceId,
          title: title.trim(),
          description: description || undefined,
        });
        if (live) setSimilar(body(r) ?? []);
      } catch {
        if (live) setSimilar([]);
      }
    }, 700);
    return () => {
      live = false;
      clearTimeout(handle);
    };
  }, [status, ready, workspaceId, title, description]);

  if (!status) return null;

  const fail = (e: unknown) => {
    const kind = errorKind(e);
    toast.danger(
      serverMessage(e) ?? (kind === 'generic' ? t('unavailable') : te(kind)),
    );
  };

  const suggestFields = async () => {
    if (!cardTypeId) return;
    setBusy('fields');
    try {
      const r = await suggestDraftFields({
        workspace_id: workspaceId,
        title: title.trim(),
        description: description || undefined,
        card_type_id: cardTypeId,
        filled,
      });
      setSuggested(body(r) ?? []);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  const draft = async () => {
    if (!descriptionSlug) return;
    setBusy('draft');
    try {
      const r = body(await draftDescription({ title: title.trim(), kind }));
      if (r) onApply(descriptionSlug, r.document);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  // Derived, not reset in the effect: a title that is too short shows nothing.
  const shown = ready ? similar : [];
  const canDraft = !!descriptionSlug && !description.trim();
  return (
    <div
      className="flex flex-col gap-2 rounded-md border border-separator bg-surface-secondary p-3"
      data-testid="ai-create"
    >
      <p className="flex items-center gap-1.5 text-xs font-medium text-muted">
        <Sparkles size={12} aria-hidden />
        {t('menu')}
      </p>
      {shown.length > 0 && (
        <div data-testid="create-similar">
          <p className="mb-1 text-xs text-muted">{t('similarWhileTyping')}</p>
          <ul className="flex flex-col">
            {shown.slice(0, 3).map((r) => (
              <li key={r.card_id} className="flex items-center gap-2 text-sm">
                <Link
                  href={routes.browse(r.key)}
                  target="_blank"
                  className="flex min-w-0 flex-1 items-center gap-2 py-1 hover:underline"
                >
                  <span className="shrink-0 text-xs text-muted">{r.key}</span>
                  <span className="truncate">{r.title}</span>
                </Link>
                {r.relation === 'duplicate' && (
                  <Lozenge kind="progress">{t('relation.duplicate')}</Lozenge>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="secondary"
          isDisabled={!ready || !cardTypeId}
          isPending={busy === 'fields'}
          onPress={suggestFields}
          data-testid="ai-suggest-fields"
        >
          {busy === 'fields' ? <Spinner size="sm" /> : <Sparkles size={14} />}
          {t('fields')}
        </Button>
        {canDraft && (
          <Button
            size="sm"
            variant="secondary"
            isDisabled={!ready}
            isPending={busy === 'draft'}
            onPress={draft}
            data-testid="ai-draft"
          >
            {busy === 'draft' ? <Spinner size="sm" /> : <Sparkles size={14} />}
            {t('draftDescription')}
          </Button>
        )}
      </div>
      {suggested &&
        (suggested.length === 0 ? (
          <p className="text-xs text-muted">{t('noFields')}</p>
        ) : (
          <ul
            className="flex flex-col gap-1"
            data-testid="ai-field-suggestions"
          >
            {suggested.map((f) => (
              <li key={f.slug} className="flex items-center gap-2 text-sm">
                <span className="min-w-0 flex-1">
                  <span className="text-muted">{f.name}: </span>
                  <strong>
                    {Array.isArray(f.value)
                      ? f.value.join(', ')
                      : String(f.value)}
                  </strong>
                </span>
                <Button
                  size="sm"
                  variant="secondary"
                  onPress={() => {
                    onApply(f.slug, f.value);
                    setSuggested(
                      (s) => s?.filter((x) => x.slug !== f.slug) ?? s,
                    );
                  }}
                >
                  {t('apply')}
                </Button>
              </li>
            ))}
          </ul>
        ))}
    </div>
  );
}
