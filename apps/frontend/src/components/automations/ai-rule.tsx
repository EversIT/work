'use client';

import { Button } from '@heroui/react';
import { Sparkles } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { type ComponentProps, useState } from 'react';
import {
  explainRule,
  parseRuleText,
  type RuleLike,
  type RuleLookups,
} from '../../lib/automation-text';
import { useAiStatus } from '../../lib/ai';
import { TextInput } from '../ui/inputs';

/**
 * The bar at the top of the rule builder: "Describe your rule" turns a sentence into a draft the
 * person reviews step by step, and "Explain this rule" says in one sentence what the rule does.
 * Rules of thumb, not a model: it understands the usual ways to say a trigger and an action.
 */
function AiRuleBarPanel({
  rule,
  lookups,
  initialText,
  onDraft,
}: {
  /** The rule as it stands in the builder. */
  rule: RuleLike;
  lookups: RuleLookups;
  /** A sentence to draft from as soon as the lookups are there (from "Create with AI"). */
  initialText?: string;
  onDraft: (rule: RuleLike) => void;
}) {
  const t = useTranslations('automations');
  const [text, setText] = useState(initialText ?? '');
  const [result, setResult] = useState<{
    understood: string[];
    unknown: string[];
    none: boolean;
  } | null>(null);
  const [explained, setExplained] = useState<string | null>(null);
  const [auto, setAuto] = useState(false);

  const draft = (sentence: string) => {
    const d = parseRuleText(sentence, lookups);
    if (!d) return setResult({ understood: [], unknown: [], none: true });
    onDraft(d.rule);
    setExplained(null);
    setResult({ understood: d.understood, unknown: d.unknown, none: false });
  };
  // A sentence that came with the link is drafted once, when the fields, statuses and people it refers to are loaded.
  if (
    initialText &&
    !auto &&
    lookups.fields.length > 0 &&
    lookups.members.length > 0
  ) {
    setAuto(true);
    draft(initialText);
  }

  return (
    <div
      className="flex flex-col gap-2 border-b border-separator bg-surface-secondary px-4 py-3 md:px-6"
      data-testid="ai-rule-bar"
    >
      <form
        className="flex flex-wrap items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (text.trim()) draft(text);
        }}
      >
        <div className="min-w-64 flex-1">
          <TextInput
            label={t('describeRule')}
            placeholder={t('describePlaceholder')}
            value={text}
            onChange={setText}
            maxLength={500}
          />
        </div>
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          isDisabled={!text.trim()}
          data-testid="draft-rule"
        >
          <Sparkles size={14} />
          {t('draftRule')}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onPress={() => setExplained(explainRule(rule, lookups, t))}
          data-testid="explain-rule"
        >
          {t('explainRule')}
        </Button>
      </form>
      {explained && (
        <p className="text-sm" data-testid="rule-explained">
          {explained}
        </p>
      )}
      {result?.none && (
        <p className="text-sm text-muted">{t('describeNone')}</p>
      )}
      {result && !result.none && (
        <p className="text-xs text-muted" data-testid="rule-understood">
          {t('understood')}: {result.understood.join(' → ')}
          {result.unknown.length > 0 && (
            <span className="text-danger">
              {' '}
              · {t('notUnderstood')}: {result.unknown.join(', ')}
            </span>
          )}
        </p>
      )}
    </div>
  );
}

/** Describing a rule in a sentence is an AI feature: the bar appears only while a model is set up. */
export function AiRuleBar(props: ComponentProps<typeof AiRuleBarPanel>) {
  return useAiStatus() ? <AiRuleBarPanel {...props} /> : null;
}
