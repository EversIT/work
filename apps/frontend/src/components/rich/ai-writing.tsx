'use client';

import type { Editor, JSONContent } from '@tiptap/core';
import { Button, toast } from '@heroui/react';
import { useTranslations } from 'next-intl';
import { useState } from 'react';
import { rewriteText } from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import type { RewriteOut } from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { isEmptyDoc } from '../../lib/rich';
import { JiraDialog } from '../ui/jira-dialog';
import { RichView } from './rich-view';

const ACTIONS = [
  'improve',
  'fix_spelling',
  'shorten',
  'expand',
  'friendly',
  'professional',
] as const;

type Proposal = {
  result: RewriteOut;
  /** The selection the proposal is for, or `null` for the whole text. */
  range: { from: number; to: number } | null;
  /** What was sent: a selection's text, or the whole document. */
  before: string | JSONContent;
};

/**
 * "Write with AI" for the editor bar: improve, shorten, fix, change the tone or translate the selection
 * (or the whole text when nothing is selected). The result is shown next to the original with Accept and
 * Discard; nothing is replaced until the person accepts.
 */
export function AiWriting({ editor }: { editor: Editor }) {
  const t = useTranslations('ai');
  const te = useTranslations('errors');
  const status = useAiStatus();
  const [busy, setBusy] = useState(false);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  if (!status) return null;

  const translate = status.languages;

  const fail = (e: unknown) => {
    const kind = errorKind(e);
    toast.danger(
      serverMessage(e) ?? (kind === 'generic' ? t('unavailable') : te(kind)),
    );
  };

  const run = async (choice: string) => {
    const [action, language] = choice.split(':');
    const { from, to } = editor.state.selection;
    const partial = from !== to;
    const text = partial ? editor.state.doc.textBetween(from, to, '\n\n') : '';
    const doc = editor.getJSON();
    if (partial ? !text.trim() : isEmptyDoc(doc)) {
      toast.warning(t('nothingToWrite'));
      return;
    }
    setBusy(true);
    try {
      const res = body(
        await rewriteText({
          action,
          language,
          ...(partial ? { text } : { document: doc }),
        }),
      );
      if (res)
        setProposal({
          result: res,
          range: partial ? { from, to } : null,
          before: partial ? text : doc,
        });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const accept = () => {
    if (!proposal) return;
    const { range, before, result } = proposal;
    const nodes = ((result.document as JSONContent).content ??
      []) as JSONContent[];
    // The text may have been edited while the proposal was open: then it no longer fits.
    const unchanged = range
      ? editor.state.doc.textBetween(range.from, range.to, '\n\n') === before
      : JSON.stringify(editor.getJSON()) === JSON.stringify(before);
    if (!unchanged) {
      toast.warning(t('changed'));
    } else if (range) {
      // One paragraph replacing a selection inside a paragraph stays inline.
      const inline =
        nodes.length === 1 && nodes[0].type === 'paragraph'
          ? (nodes[0].content ?? [])
          : nodes;
      editor.chain().focus().insertContentAt(range, inline).run();
    } else {
      editor
        .chain()
        .focus()
        .setContent(result.document as JSONContent)
        .run();
    }
    setProposal(null);
  };

  return (
    // The dialog is rendered through a portal but its events still bubble through here to the toolbar,
    // which cancels `mousedown` to keep the editor's focus: stop them before they get there.
    <span
      role="presentation"
      onMouseDown={(e) => e.stopPropagation()}
      className="contents"
    >
      <select
        aria-label={t('writing')}
        data-testid="ai-writing"
        value=""
        disabled={busy}
        onMouseDown={(e) => e.stopPropagation()}
        onChange={(e) => e.target.value && run(e.target.value)}
        className="h-8 w-28 rounded-sm bg-transparent px-2 text-sm text-foreground hover:bg-default-hover focus:outline-none"
      >
        <option value="">{busy ? '…' : `✦ ${t('writing')}`}</option>
        {ACTIONS.map((a) => (
          <option key={a} value={a}>
            {t(`rewrite.${a}`)}
          </option>
        ))}
        {translate.map((l) => (
          <option key={l.code} value={`translate:${l.code}`}>
            {t('translateTo', { language: l.name })}
          </option>
        ))}
      </select>
      <span aria-hidden className="mx-1 h-5 w-px bg-border" />
      {proposal && (
        <JiraDialog
          isOpen
          onClose={() => setProposal(null)}
          title={t('rewriteTitle')}
          size="md"
          testId="ai-writing-dialog"
          footer={
            <>
              <Button
                variant="primary"
                onPress={accept}
                data-testid="ai-accept"
              >
                {t('accept')}
              </Button>
              <Button variant="tertiary" onPress={() => setProposal(null)}>
                {t('discard')}
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-4 pb-2">
            <div>
              <p className="mb-1 text-xs font-medium text-muted">
                {t('before')}
              </p>
              <div className="rounded-md border border-separator p-3">
                {typeof proposal.before === 'string' ? (
                  <p className="whitespace-pre-wrap text-sm">
                    {proposal.before}
                  </p>
                ) : (
                  <RichView value={proposal.before} />
                )}
              </div>
            </div>
            <div>
              <p className="mb-1 text-xs font-medium text-muted">
                {t('after')}
              </p>
              <div className="rounded-md border border-accent p-3">
                <RichView value={proposal.result.document} />
              </div>
            </div>
            <p className="text-xs text-muted">{t('reviewHint')}</p>
          </div>
        </JiraDialog>
      )}
    </span>
  );
}
