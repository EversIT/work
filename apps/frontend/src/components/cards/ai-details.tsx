'use client';

import { Button, Spinner, toast } from '@heroui/react';
import { Check, Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { suggestCardFields } from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import type { FieldSuggestion } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';

/** Asks for values for the empty choice fields (priority, labels, ...) from similar issues. */
export function SuggestFieldsButton({
  cardId,
  onResult,
}: {
  cardId: string;
  onResult: (s: FieldSuggestion[]) => void;
}) {
  const t = useTranslations('ai');
  const te = useTranslations('errors');
  const status = useAiStatus();
  const [busy, setBusy] = useState(false);
  if (!status) return null;
  const run = async () => {
    setBusy(true);
    try {
      const r = body(await suggestCardFields(cardId)) ?? [];
      onResult(r);
      if (r.length === 0) toast.info(t('noFields'));
    } catch (e) {
      const kind = errorKind(e);
      toast.danger(
        serverMessage(e) ?? (kind === 'generic' ? t('unavailable') : te(kind)),
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <Button
      size="sm"
      variant="ghost"
      onPress={run}
      isPending={busy}
      data-testid="suggest-details"
    >
      {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
      {t('suggest')}
    </Button>
  );
}

/** A suggested value under its field, dotted so it reads as "not set yet", with one click to accept. */
export function InlineSuggestion({
  suggestion,
  onAccept,
}: {
  suggestion: FieldSuggestion;
  onAccept: () => void;
}) {
  const t = useTranslations('ai');
  const value = Array.isArray(suggestion.value)
    ? suggestion.value.join(', ')
    : String(suggestion.value);
  return (
    <div
      className="mx-3 mb-1 flex items-center gap-2 rounded-sm border border-dotted border-accent px-2 py-1 text-xs"
      data-testid={`suggested-${suggestion.slug}`}
      title={t('basedOn', {
        count: suggestion.based_on,
        percent: Math.round(suggestion.confidence * 100),
      })}
    >
      <Sparkles size={12} aria-hidden className="shrink-0 text-accent" />
      <span className="min-w-0 flex-1 truncate">{value}</span>
      <button
        type="button"
        onClick={onAccept}
        className="flex items-center gap-1 text-accent hover:underline"
        aria-label={t('apply')}
      >
        <Check size={12} aria-hidden />
        {t('apply')}
      </button>
    </div>
  );
}
