'use client';

import {
  Button,
  Checkbox,
  Dropdown,
  Spinner,
  buttonVariants,
  toast,
} from '@heroui/react';
import { useQueryClient } from '@tanstack/react-query';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import { useState } from 'react';
import {
  draftDescription,
  relatedCards,
  rewriteCardDescription,
  suggestCardFields,
  suggestSubtasks,
  summarizeCard,
} from '../../api/generated/ai/ai';
import { useAiStatus } from '../../lib/ai';
import { createCard, updateCard } from '../../api/generated/cards/cards';
import type {
  CardOut,
  FieldSuggestion,
  RelatedOut,
  RewriteOut,
  SummaryOut,
} from '../../api/generated/model';
import { body, errorKind, serverMessage } from '../../lib/api';
import { routes } from '../../lib/routes';
import { Lozenge } from '../jira/issue-ui';
import { RichView } from '../rich/rich-view';
import { useWorkspaceCtx } from '../shell/workspace-context';
import { JiraDialog } from '../ui/jira-dialog';
import { SimpleSelect } from '../ui/inputs';

type Rewrite =
  | 'improve'
  | 'fix_spelling'
  | 'shorten'
  | 'expand'
  | 'professional'
  | 'friendly';
const REWRITES: Rewrite[] = [
  'improve',
  'fix_spelling',
  'shorten',
  'expand',
  'professional',
  'friendly',
];

type View =
  | { kind: 'summary'; data: SummaryOut }
  | { kind: 'related'; data: RelatedOut[] }
  | { kind: 'fields'; data: FieldSuggestion[] }
  | { kind: 'subtasks'; data: string[] }
  | { kind: 'rewrite'; data: RewriteOut; action: string }
  | { kind: 'translate' };

/**
 * The assistant menu of an issue, like Jira's: summarize, find related issues, suggest field values
 * and child issues, improve or translate the description. Everything it returns is a suggestion;
 * nothing changes until the person presses apply.
 */
export function AiAssistant({
  card,
  canWrite,
  descriptionSlug,
  titleSlug,
  title: issueText,
  kind,
  shortcut,
}: {
  card: CardOut;
  canWrite: boolean;
  /** The rich-text description field, when the issue type has one. */
  descriptionSlug?: string;
  titleSlug?: string;
  /** The issue's title and the name of its type: the input of a drafted description. */
  title?: string;
  kind?: string;
  /**
   * Instead of the menu, one button that runs one action and shows its result in the same dialog: the
   * shortcut under the description ("draft" while it is empty, "improve" once it is not) and the
   * "suggest" button next to the child issues.
   */
  shortcut?: 'draft' | 'improve' | 'subtasks';
}) {
  const t = useTranslations('ai');
  const te = useTranslations('errors');
  const qc = useQueryClient();
  const ctx = useWorkspaceCtx();
  const status = useAiStatus();
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<View | null>(null);
  const [language, setLanguage] = useState('en');
  const [picked, setPicked] = useState<Set<number>>(new Set());

  // Without a status the server has no assistant (or the demo has none): show nothing.
  if (!status) return null;

  const fail = (e: unknown) => {
    const kind = errorKind(e);
    toast.danger(
      serverMessage(e) ?? (kind === 'generic' ? t('unavailable') : te(kind)),
    );
  };
  const run = async (fn: () => Promise<View | undefined>) => {
    setBusy(true);
    try {
      const v = await fn();
      if (v) {
        setPicked(
          new Set(v.kind === 'subtasks' ? v.data.map((_, i) => i) : []),
        );
        setView(v);
      }
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  const done = async (message: string) => {
    await qc.invalidateQueries();
    toast.success(message);
  };

  const hasDescription = !!descriptionSlug && !!card.data[descriptionSlug];
  const languages = status.languages;
  const lang = languages.some((l) => l.code === language)
    ? language
    : languages[0]?.code;
  const canTranslate = languages.length > 0 && canWrite && hasDescription;
  const translate = () =>
    run(async () => {
      if (!lang) return undefined;
      const r = body(
        await rewriteCardDescription(card.id, {
          action: 'translate',
          language: lang,
        }),
      );
      return r && { kind: 'rewrite', data: r, action: 'translate' };
    });

  const onAction = (key: React.Key) => {
    const k = String(key);
    if (k === 'draft')
      return run(async () => {
        const r = body(
          await draftDescription({ title: issueText ?? '', kind }),
        );
        return r && { kind: 'rewrite', data: r, action: 'draft' };
      });
    if (k === 'summarize')
      return run(async () => {
        const r = body(await summarizeCard(card.id, {}));
        return r && { kind: 'summary', data: r };
      });
    if (k === 'related')
      return run(async () => {
        const r = body(await relatedCards(card.id));
        return r && { kind: 'related', data: r };
      });
    if (k === 'fields')
      return run(async () => {
        const r = body(await suggestCardFields(card.id));
        return r && { kind: 'fields', data: r };
      });
    if (k === 'subtasks')
      return run(async () => {
        const r = body(await suggestSubtasks(card.id));
        return r && { kind: 'subtasks', data: r.suggestions };
      });
    if (k === 'translate') return setView({ kind: 'translate' });
    if (REWRITES.includes(k as Rewrite))
      return run(async () => {
        const r = body(await rewriteCardDescription(card.id, { action: k }));
        return r && { kind: 'rewrite', data: r, action: k };
      });
  };

  const applyDescription = async (doc: unknown) => {
    if (!descriptionSlug) return;
    try {
      await updateCard(card.id, { data: { [descriptionSlug]: doc } });
      setView(null);
      await done(t('applied'));
    } catch (e) {
      fail(e);
    }
  };
  const applyField = async (f: FieldSuggestion) => {
    try {
      await updateCard(card.id, { data: { [f.slug]: f.value } });
      setView((v) =>
        v?.kind === 'fields'
          ? { ...v, data: v.data.filter((x) => x.slug !== f.slug) }
          : v,
      );
      await done(t('applied'));
    } catch (e) {
      fail(e);
    }
  };
  const createChildren = async (titles: string[]) => {
    const primary = card.lists[0];
    const type =
      ctx.cardTypes.find((c) => c.icon === 'subtask') ??
      ctx.cardTypes.find((c) => c.id === card.card_type_id);
    const slug = type?.fields.find((f) => f.is_title)?.slug ?? titleSlug;
    if (!primary || !type || !slug) return;
    try {
      for (const title of titles)
        await createCard(primary.list_id, {
          card_type_id: type.id,
          parent_id: card.id,
          data: { [slug]: title },
        });
      setView(null);
      await done(t('created', { count: titles.length }));
    } catch (e) {
      fail(e);
    }
  };

  const modelItems = canWrite;
  const title =
    view?.kind === 'summary'
      ? t('summaryTitle')
      : view?.kind === 'related'
        ? t('relatedTitle')
        : view?.kind === 'fields'
          ? t('fieldsTitle')
          : view?.kind === 'subtasks'
            ? t('subtasksTitle')
            : view?.kind === 'translate'
              ? t('translate')
              : t('rewriteTitle');

  const empty = !!descriptionSlug && !hasDescription;
  const shortcutLabel = {
    draft: t('draftDescription'),
    improve: t('rewrite.improve'),
    subtasks: t('subtasks'),
  };
  const shortcutOk =
    !!shortcut &&
    canWrite &&
    (shortcut === 'draft'
      ? empty && !!issueText?.trim()
      : shortcut === 'improve'
        ? hasDescription
        : true);

  return (
    <>
      {shortcut ? (
        shortcutOk && (
          <Button
            size="sm"
            variant="secondary"
            isPending={busy}
            onPress={() => onAction(shortcut)}
            data-testid={`ai-shortcut-${shortcut}`}
          >
            {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
            {shortcutLabel[shortcut]}
          </Button>
        )
      ) : (
        <Dropdown>
          <Dropdown.Trigger
            className={buttonVariants({ variant: 'tertiary', size: 'sm' })}
            aria-label={t('menu')}
            data-testid="ai-menu"
            isDisabled={busy}
          >
            {busy ? <Spinner size="sm" /> : <Sparkles size={14} />}
            {t('menu')}
          </Dropdown.Trigger>
          <Dropdown.Popover>
            <Dropdown.Menu aria-label={t('menu')} onAction={onAction}>
              <Dropdown.Item id="summarize" textValue={t('summarize')}>
                {t('summarize')}
              </Dropdown.Item>
              <Dropdown.Item id="related" textValue={t('related')}>
                {t('related')}
              </Dropdown.Item>
              {canWrite ? (
                <Dropdown.Item id="fields" textValue={t('fields')}>
                  {t('fields')}
                </Dropdown.Item>
              ) : null}
              {modelItems ? (
                <Dropdown.Item id="subtasks" textValue={t('subtasks')}>
                  {t('subtasks')}
                </Dropdown.Item>
              ) : null}
              {modelItems && empty && issueText?.trim() ? (
                <Dropdown.Item id="draft" textValue={t('draftDescription')}>
                  {t('draftDescription')}
                </Dropdown.Item>
              ) : null}
              {modelItems && hasDescription
                ? REWRITES.map((r) => (
                    <Dropdown.Item key={r} id={r} textValue={t(`rewrite.${r}`)}>
                      {t(`rewrite.${r}`)}
                    </Dropdown.Item>
                  ))
                : null}
              {canTranslate ? (
                <Dropdown.Item id="translate" textValue={t('translate')}>
                  {t('translate')}
                </Dropdown.Item>
              ) : null}
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
      )}

      {view && (
        <JiraDialog
          isOpen
          onClose={() => setView(null)}
          title={title}
          size="md"
          testId="ai-dialog"
          footer={
            <>
              {view.kind === 'rewrite' && (
                <Button
                  variant="primary"
                  onPress={() => applyDescription(view.data.document)}
                  data-testid="ai-apply"
                >
                  {t('replaceDescription')}
                </Button>
              )}
              {view.kind === 'fields' && view.data.length > 1 && (
                <Button
                  variant="primary"
                  onPress={async () => {
                    const patch = Object.fromEntries(
                      view.data.map((f) => [f.slug, f.value]),
                    );
                    try {
                      await updateCard(card.id, { data: patch });
                      setView(null);
                      await done(t('applied'));
                    } catch (e) {
                      fail(e);
                    }
                  }}
                >
                  {t('applyAll')}
                </Button>
              )}
              {view.kind === 'subtasks' && view.data.length > 0 && (
                <Button
                  variant="primary"
                  isDisabled={picked.size === 0}
                  onPress={() =>
                    createChildren(view.data.filter((_, i) => picked.has(i)))
                  }
                  data-testid="ai-create-children"
                >
                  {t('createChildren', { count: picked.size })}
                </Button>
              )}
              {view.kind === 'translate' && (
                <Button
                  variant="primary"
                  isDisabled={busy}
                  onPress={translate}
                  data-testid="ai-translate-run"
                >
                  {t('translateRun')}
                </Button>
              )}
              <Button variant="tertiary" onPress={() => setView(null)}>
                {t('close')}
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-3 pb-2" data-testid="ai-result">
            {view.kind === 'summary' && (
              <>
                <RichView value={view.data.document} />
                <p className="text-xs text-muted">
                  {view.data.source === 'model'
                    ? t('byModel')
                    : t('byAnalysis')}
                  {view.data.keywords.length > 0 &&
                    ` · ${t('topics')}: ${view.data.keywords.join(', ')}`}
                </p>
              </>
            )}
            {view.kind === 'related' &&
              (view.data.length === 0 ? (
                <p className="text-sm text-muted">{t('noRelated')}</p>
              ) : (
                <ul className="divide-y divide-separator">
                  {view.data.map((r) => (
                    <li key={r.card_id} className="flex items-start gap-3 py-2">
                      <div className="min-w-0 flex-1">
                        <Link
                          href={routes.browse(r.key)}
                          className="text-sm hover:underline"
                          onClick={() => setView(null)}
                        >
                          <span className="mr-2 text-xs text-muted">
                            {r.key}
                          </span>
                          {r.title}
                        </Link>
                        {r.shared.length > 0 && (
                          <p className="text-xs text-muted">
                            {t('shared')}: {r.shared.join(', ')}
                          </p>
                        )}
                      </div>
                      <Lozenge
                        kind={r.relation === 'duplicate' ? 'progress' : 'todo'}
                      >
                        {t(
                          `relation.${r.relation === 'duplicate' ? 'duplicate' : 'related'}`,
                        )}
                      </Lozenge>
                    </li>
                  ))}
                </ul>
              ))}
            {view.kind === 'fields' &&
              (view.data.length === 0 ? (
                <p className="text-sm text-muted">{t('noFields')}</p>
              ) : (
                <ul className="divide-y divide-separator">
                  {view.data.map((f) => (
                    <li key={f.slug} className="flex items-center gap-3 py-2">
                      <div className="min-w-0 flex-1 text-sm">
                        <span className="text-muted">{f.name}: </span>
                        <strong>
                          {Array.isArray(f.value)
                            ? f.value.join(', ')
                            : String(f.value)}
                        </strong>
                        <p className="text-xs text-muted">
                          {t('basedOn', {
                            count: f.based_on,
                            percent: Math.round(f.confidence * 100),
                          })}
                        </p>
                      </div>
                      <Button
                        size="sm"
                        variant="secondary"
                        onPress={() => applyField(f)}
                      >
                        {t('apply')}
                      </Button>
                    </li>
                  ))}
                </ul>
              ))}
            {view.kind === 'subtasks' &&
              (view.data.length === 0 ? (
                <p className="text-sm text-muted">{t('noSubtasks')}</p>
              ) : (
                <ul className="flex flex-col gap-2">
                  {view.data.map((s, i) => (
                    <li key={i}>
                      <Checkbox
                        isSelected={picked.has(i)}
                        onChange={(on: boolean) =>
                          setPicked((p) => {
                            const n = new Set(p);
                            if (on) n.add(i);
                            else n.delete(i);
                            return n;
                          })
                        }
                      >
                        <Checkbox.Control>
                          <Checkbox.Indicator />
                        </Checkbox.Control>
                        <Checkbox.Content>{s}</Checkbox.Content>
                      </Checkbox>
                    </li>
                  ))}
                </ul>
              ))}
            {view.kind === 'rewrite' && (
              <>
                <RichView value={view.data.document} />
                <p className="text-xs text-muted">{t('reviewHint')}</p>
              </>
            )}
            {view.kind === 'translate' && (
              <SimpleSelect
                label={t('language')}
                value={lang}
                onChange={(v) => v && setLanguage(v)}
                options={languages.map((l) => ({
                  value: l.code,
                  label: l.name,
                }))}
                testId="ai-language"
              />
            )}
          </div>
        </JiraDialog>
      )}
    </>
  );
}
