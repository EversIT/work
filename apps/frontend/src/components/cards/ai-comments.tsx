'use client';

import { Button, Spinner, toast } from '@heroui/react';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import {
  checkTone,
  suggestReplies,
  summarizeCard,
} from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import type { SummaryOut, ToneOut } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { isEmptyDoc } from '../../lib/rich';
import { RichView } from '../rich/rich-view';
import { JiraDialog } from '../ui/jira-dialog';

/** Comments in a thread above which a summary is offered. */
export const SUMMARY_FROM = 8;

function useFail() {
  const t = useTranslations('ai');
  const te = useTranslations('errors');
  return (e: unknown) => {
    const kind = errorKind(e);
    toast.danger(
      serverMessage(e) ?? (kind === 'generic' ? t('unavailable') : te(kind)),
    );
  };
}

/** "Summarize": a long thread boiled down to what was decided, what is open and what comes next. */
export function ThreadSummary({ cardId }: { cardId: string }) {
  const t = useTranslations('ai');
  const status = useAiStatus();
  const fail = useFail();
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<SummaryOut | null>(null);
  if (!status) return null;
  const run = async () => {
    setBusy(true);
    try {
      const r = body(await summarizeCard(cardId, {}));
      if (r) setSummary(r);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  if (summary)
    return (
      <div
        className="rounded-md border border-separator bg-surface-secondary p-3"
        data-testid="thread-summary"
      >
        <div className="mb-2 flex items-center justify-between">
          <h4 className="flex items-center gap-1.5 text-sm font-semibold">
            <Sparkles size={14} aria-hidden />
            {t('threadTitle')}
          </h4>
          <Button size="sm" variant="ghost" onPress={() => setSummary(null)}>
            {t('dismiss')}
          </Button>
        </div>
        <RichView value={summary.document} />
        <p className="mt-2 text-xs text-muted">
          {summary.source === 'model' ? t('byModel') : t('byAnalysis')}
        </p>
      </div>
    );
  return (
    <Button
      size="sm"
      variant="secondary"
      className="self-start"
      isPending={busy}
      onPress={run}
      data-testid="summarize-thread"
    >
      {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
      {t('summarizeThread')}
    </Button>
  );
}

/**
 * Quick replies under the comment box, like Jira's. Three fixed ones are always there; with the language
 * model a button writes replies that fit the latest comment. A click only fills the editor: the person
 * edits and sends.
 */
export function ReplyChips({
  cardId,
  onPick,
}: {
  cardId: string;
  onPick: (text: string) => void;
}) {
  const t = useTranslations('ai');
  const status = useAiStatus();
  const fail = useFail();
  const [busy, setBusy] = useState(false);
  const [written, setWritten] = useState<string[]>([]);
  const fixed = [t('replyInfo'), t('replyStatus'), t('replyThanks')];
  const suggest = async () => {
    setBusy(true);
    try {
      const r = body(await suggestReplies(cardId));
      if (r) setWritten(r.suggestions);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      className="flex flex-wrap items-center gap-1.5"
      data-testid="reply-chips"
    >
      {[...written, ...fixed].map((text) => (
        <button
          key={text}
          type="button"
          onClick={() => onPick(text)}
          className="max-w-full truncate rounded-full border border-separator px-3 py-1 text-xs text-foreground hover:bg-default-hover"
          title={text}
        >
          {text}
        </button>
      ))}
      {status && (
        <Button
          size="sm"
          variant="ghost"
          isPending={busy}
          onPress={suggest}
          data-testid="suggest-replies"
        >
          {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
          {t('suggestReplies')}
        </Button>
      )}
    </div>
  );
}

/** "Check tone": is the comment friendly and constructive? If not, a friendlier version to take or leave. */
export function ToneCheck({
  doc,
  onUse,
}: {
  doc: unknown;
  onUse: (document: unknown) => void;
}) {
  const t = useTranslations('ai');
  const status = useAiStatus();
  const fail = useFail();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ToneOut | null>(null);
  if (!status) return null;
  const run = async () => {
    if (isEmptyDoc(doc)) return;
    setBusy(true);
    try {
      const r = body(
        await checkTone({ document: doc as Record<string, unknown> }),
      );
      if (!r) return;
      if (r.ok) toast.success(t('toneOk'));
      else setResult(r);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Button
        size="sm"
        variant="ghost"
        isPending={busy}
        isDisabled={isEmptyDoc(doc)}
        onPress={run}
        data-testid="check-tone"
      >
        {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
        {t('checkTone')}
      </Button>
      {result && (
        <JiraDialog
          isOpen
          onClose={() => setResult(null)}
          title={t('toneTitle')}
          size="md"
          testId="tone-dialog"
          footer={
            <>
              <Button
                variant="primary"
                onPress={() => {
                  onUse(result.document);
                  setResult(null);
                }}
              >
                {t('useSuggestion')}
              </Button>
              <Button variant="tertiary" onPress={() => setResult(null)}>
                {t('keepMine')}
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-2 pb-2">
            <p className="text-sm text-muted">{t('toneHint')}</p>
            <div className="rounded-md border border-accent p-3">
              <RichView value={result.document ?? result.suggestion} />
            </div>
          </div>
        </JiraDialog>
      )}
    </>
  );
}
