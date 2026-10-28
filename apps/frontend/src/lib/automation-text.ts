import type { FieldOut } from '../api/generated/model';

/** A rule in the shape the builder edits. */
export type RuleLike = {
  trigger_type: string;
  trigger_config: Record<string, unknown>;
  steps: { action_type: string; config: Record<string, unknown> }[];
};

export type RuleLookups = {
  fields: FieldOut[];
  stages: { id: string; name: string }[];
  members: { user_id: string; first_name: string; last_name: string }[];
};

/** A translator: `useTranslations('automations')`, or anything with the same call shape. */
export type T = (
  key: string,
  values?: Record<string, string | number>,
) => string;

const person = (lk: RuleLookups, id: unknown) => {
  const m = lk.members.find((x) => x.user_id === id);
  return m ? `${m.first_name} ${m.last_name}` : '';
};
const stageName = (lk: RuleLookups, ids: unknown) => {
  const id = Array.isArray(ids) ? ids[0] : ids;
  return lk.stages.find((s) => s.id === id)?.name ?? '';
};
const fieldName = (lk: RuleLookups, slug: unknown) =>
  lk.fields.find((f) => f.slug === slug)?.name ?? String(slug ?? '');
const show = (v: unknown): string =>
  v === null || v === undefined || v === ''
    ? ''
    : Array.isArray(v)
      ? v.map(show).join(', ')
      : String(v);

function when(rule: RuleLike, lk: RuleLookups, t: T): string {
  const c = rule.trigger_config;
  switch (rule.trigger_type) {
    case 'card_created':
      return t('explain.card_created');
    case 'comment_created':
      return t('explain.comment_created');
    case 'webhook':
      return t('explain.webhook');
    case 'field_updated': {
      const field = fieldName(lk, c.field);
      if (!field) return t('explain.field_any');
      if (c.from && c.to)
        return t('explain.field_from_to', {
          field,
          from: show(c.from),
          to: show(c.to),
        });
      if (c.to) return t('explain.field_to', { field, to: show(c.to) });
      return t('explain.field_changes', { field });
    }
    case 'stage_changed': {
      const from = stageName(lk, c.from);
      const to = stageName(lk, c.to);
      if (from && to) return t('explain.stage_from_to', { from, to });
      if (to) return t('explain.stage_to', { to });
      if (from) return t('explain.stage_from', { from });
      return t('explain.stage_any');
    }
    case 'assignee_added':
    case 'assignee_removed': {
      const user = person(lk, c.user_id);
      const key = `explain.${rule.trigger_type}${user ? '_user' : ''}`;
      return t(key, { user });
    }
    case 'date_arrives': {
      const field = fieldName(lk, c.field ?? 'due_at');
      const minutes =
        typeof c.offset_minutes === 'number' ? c.offset_minutes : 0;
      if (minutes === 0) return t('explain.date_on', { field });
      const hours = Math.abs(minutes / 60);
      return t(minutes < 0 ? 'explain.date_before' : 'explain.date_after', {
        hours,
        field,
      });
    }
    default:
      return t('explain.unknown_trigger');
  }
}

function host(url: unknown): string {
  try {
    return new URL(String(url)).host;
  } catch {
    return '';
  }
}

function then(step: RuleLike['steps'][number], lk: RuleLookups, t: T): string {
  const c = step.config;
  switch (step.action_type) {
    case 'set_field':
      return t('explain.set_field', {
        field: fieldName(lk, c.field),
        value: show(c.value) || '–',
      });
    case 'set_stage':
      return t('explain.set_stage', {
        stage: stageName(lk, c.stage_id) || '?',
      });
    case 'change_assignee': {
      const add = Array.isArray(c.add) ? person(lk, c.add[0]) : '';
      const remove = Array.isArray(c.remove) ? person(lk, c.remove[0]) : '';
      if (add && remove) return t('explain.swap_assignee', { add, remove });
      if (add) return t('explain.add_assignee', { user: add });
      if (remove) return t('explain.remove_assignee', { user: remove });
      return t('explain.change_assignee');
    }
    case 'create_comment': {
      const text = show(c.content).replace(/\s+/g, ' ');
      return t('explain.create_comment', {
        text: text.length > 60 ? `${text.slice(0, 57)}…` : text,
      });
    }
    case 'create_card':
      return t('explain.create_card');
    case 'create_sub_card':
      return t('explain.create_sub_card');
    case 'send_webhook':
      return t('explain.send_webhook', { host: host(c.url) || '?' });
    case 'send_email':
      return t('explain.send_email', {
        to: person(lk, c.to) || show(c.to) || '?',
      });
    default:
      return t('explain.unknown_action');
  }
}

/** The rule as one plain sentence: "When …, set … and move …." */
export function explainRule(rule: RuleLike, lk: RuleLookups, t: T): string {
  const actions = rule.steps.map((s) => then(s, lk, t));
  return t('explain.sentence', {
    when: when(rule, lk, t),
    actions:
      actions.length > 0
        ? actions.join(t('explain.and'))
        : t('explain.nothing'),
  });
}

// ---- failed runs --------------------------------------------------------------------------------

type Reason = { key: string; values?: Record<string, string> };

const REASONS: [RegExp, (m: RegExpMatchArray) => Reason][] = [
  [/webhook url must use https/, () => ({ key: 'https' })],
  [/not a public address/, () => ({ key: 'private' })],
  [/could not resolve the webhook host/, () => ({ key: 'resolve' })],
  [/invalid webhook url/, () => ({ key: 'badUrl' })],
  [/webhook request failed/, () => ({ key: 'request' })],
  [/webhook payload too large/, () => ({ key: 'large' })],
  [/e-mail is not configured/, () => ({ key: 'noMail' })],
  [/recipient is not a member/, () => ({ key: 'recipient' })],
  [/e-mail body too long/, () => ({ key: 'mailLong' })],
  [
    /owner (?:no longer has access|cannot write)|no owner any more/,
    () => ({ key: 'owner' }),
  ],
  [/needs a card/, () => ({ key: 'needsCard' })],
  [/not in the list of that stage/, () => ({ key: 'stageList' })],
  [/no assignee field/, () => ({ key: 'noAssignee' })],
  [/needs `(\w+)`/, (m) => ({ key: 'needs', values: { what: m[1] } })],
];

/** What a failed run's error means and what to do about it, or `null` for an error we do not know. */
export function explainFailure(
  error: string | null | undefined,
  t: T,
): string | null {
  if (!error) return null;
  for (const [re, make] of REASONS) {
    const m = error.match(re);
    if (m) {
      const r = make(m);
      return t(`fail.${r.key}`, r.values);
    }
  }
  return null;
}

/** "3 of 10 runs failed: 2 × <reason>, 1 × <other>", or that all went well. */
export function summarizeRuns(
  runs: { status: string; error?: string | null }[],
  t: T,
): string {
  const failed = runs.filter((r) => r.status === 'failed');
  if (runs.length === 0) return t('fail.noRuns');
  if (failed.length === 0) return t('fail.allFine', { total: runs.length });
  const groups = new Map<string, number>();
  for (const r of failed) {
    const why = explainFailure(r.error, t) ?? r.error ?? t('fail.unknown');
    groups.set(why, (groups.get(why) ?? 0) + 1);
  }
  const list = [...groups.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([why, n]) => `${n} × ${why}`);
  return t('fail.summary', {
    failed: failed.length,
    total: runs.length,
    reasons: list.join(' · '),
  });
}

// ---- a sentence -> a rule ----------------------------------------------------------------------

export type DraftRule = {
  rule: RuleLike;
  understood: string[];
  unknown: string[];
};

const re = (s: string) => new RegExp(s, 'iu');
const W = String.raw`(?<![\p{L}\p{N}])`;
const E = String.raw`(?![\p{L}\p{N}])`;

function findMember(lk: RuleLookups, text: string) {
  return lk.members.find((m) => {
    const names = [
      `${m.first_name} ${m.last_name}`,
      m.first_name,
      m.last_name,
    ].filter((n) => n.length >= 3);
    return names.some((n) =>
      re(`${W}${n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(text),
    );
  });
}

function findStage(lk: RuleLookups, text: string) {
  return [...lk.stages]
    .sort((a, b) => b.name.length - a.name.length)
    .find((s) =>
      re(`${W}${s.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(text),
    );
}

/**
 * Turns a sentence into a rule draft: "When an issue is created, assign it to Anna and comment 'welcome'".
 * Rules, not a model: it knows the usual ways to say a trigger (created, commented, assigned,
 * moved to a status, a field changes to a value, a date arrives) and actions (assign, comment,
 * move, set a field), in English and German. What it did not understand is returned, so the
 * person can finish the rule in the builder. `null` when there is no trigger or no action.
 */
export function parseRuleText(text: string, lk: RuleLookups): DraftRule | null {
  // Quoted text is a comment, not words to read: park it.
  const quotes: string[] = [];
  const parked = text.replace(
    /["“„'‚]([^"”“'‘]+)["”“'‘]/gu,
    (_, q: string) => `§${quotes.push(q) - 1}§`,
  );
  const m = parked.match(
    re(
      `^\\s*(?:when|if|wenn|sobald)\\s+(.*?)\\s*(?:,|;|\\bthen\\b|\\bdann\\b)\\s*(.+)$`,
    ),
  );
  if (!m) return null;
  const [, triggerText, actionText] = m;
  const understood: string[] = [];
  const unknown: string[] = [];

  let trigger: { type: string; config: Record<string, unknown> } | null = null;
  const stage = findStage(lk, triggerText);
  if (
    re(`${W}(?:unassigned|nicht mehr zugewiesen|entfernt)${E}`).test(
      triggerText,
    )
  ) {
    const who = findMember(lk, triggerText);
    trigger = {
      type: 'assignee_removed',
      config: who ? { user_id: who.user_id } : {},
    };
  } else if (re(`${W}(?:assigned|zugewiesen)${E}`).test(triggerText)) {
    const who = findMember(lk, triggerText);
    trigger = {
      type: 'assignee_added',
      config: who ? { user_id: who.user_id } : {},
    };
  } else if (
    re(`${W}(?:comment|comments|commented|kommentar|kommentiert)${E}`).test(
      triggerText,
    )
  ) {
    trigger = { type: 'comment_created', config: {} };
  } else if (re(`${W}(?:created|erstellt|angelegt)${E}`).test(triggerText)) {
    trigger = { type: 'card_created', config: {} };
  } else if (
    re(`${W}(?:due date|fällig\\w*)${E}`).test(triggerText) &&
    re(`${W}(?:arrives|reached|erreicht|ist erreicht|kommt)${E}`).test(
      triggerText,
    )
  ) {
    const due =
      lk.fields.find((f) => f.slug === 'due_at') ??
      lk.fields.find((f) => f.type === 'date' || f.type === 'date_time');
    trigger = due
      ? { type: 'date_arrives', config: { field: due.slug } }
      : null;
  } else if (
    stage &&
    re(`${W}(?:moves?|moved|verschoben|wechselt|geht|status)${E}`).test(
      triggerText,
    )
  ) {
    trigger = { type: 'stage_changed', config: { to: [stage.id] } };
  } else {
    const field = lk.fields.find((f) =>
      re(`${W}${f.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(
        triggerText,
      ),
    );
    const items =
      field && Array.isArray(field.items)
        ? (field.items as { item: unknown }[]).map((i) => String(i.item))
        : [];
    const to = items.find((i) =>
      re(`${W}${i.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(
        triggerText,
      ),
    );
    if (field && /changes|changed|ändert|geändert/i.test(triggerText)) {
      trigger = {
        type: 'field_updated',
        config: { field: field.slug, ...(to ? { to } : {}) },
      };
    }
  }
  if (!trigger) return null;
  understood.push(triggerText.trim());

  const steps: RuleLike['steps'] = [];
  // Actions are split at commas, "and" and "then", after the quotes are parked.
  for (const raw of actionText.split(
    re(`\\s*(?:,|;|\\band\\b|\\bthen\\b|\\bund\\b|\\bdann\\b)\\s*`),
  )) {
    const clause = raw.trim();
    if (!clause) continue;
    const quoted = clause.match(/§(\d+)§/);
    const q = quoted ? quotes[Number(quoted[1])] : undefined;
    let step: RuleLike['steps'][number] | null = null;
    const who = findMember(lk, clause);
    const st = findStage(lk, clause);
    if (re(`${W}(?:comment|kommentier\\w*|kommentar)${E}`).test(clause) && q) {
      step = { action_type: 'create_comment', config: { content: q } };
    } else if (re(`${W}(?:assign|zuweisen|weise)${E}`).test(clause) && who) {
      step = { action_type: 'change_assignee', config: { add: [who.user_id] } };
    } else if (
      st &&
      re(`${W}(?:move|moves|set|transition|verschiebe|setze|status)${E}`).test(
        clause,
      )
    ) {
      step = { action_type: 'set_stage', config: { stage_id: st.id } };
    } else {
      const field = lk.fields.find((f) =>
        re(`${W}${f.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(
          clause,
        ),
      );
      const items =
        field && Array.isArray(field.items)
          ? (field.items as { item: unknown }[]).map((i) => String(i.item))
          : [];
      const value = items.find((i) =>
        re(`${W}${i.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}${E}`).test(clause),
      );
      if (field && value && /\bset\b|setze|\bto\b|auf/i.test(clause)) {
        step = {
          action_type: 'set_field',
          config: {
            field: field.slug,
            value: field.multiple ? [value] : value,
          },
        };
      }
    }
    if (step) {
      steps.push(step);
      understood.push(clause.replace(/§\d+§/g, q ? `"${q}"` : ''));
    } else {
      unknown.push(clause.replace(/§\d+§/g, q ? `"${q}"` : ''));
    }
  }
  if (steps.length === 0) return null;
  return {
    rule: { trigger_type: trigger.type, trigger_config: trigger.config, steps },
    understood,
    unknown,
  };
}
