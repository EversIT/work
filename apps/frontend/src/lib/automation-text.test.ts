import { describe, expect, it } from 'vitest';
import type { FieldOut } from '../api/generated/model';
import {
  explainFailure,
  explainRule,
  parseRuleText,
  summarizeRuns,
  type RuleLookups,
  type T,
} from './automation-text';

const field = (o: Partial<FieldOut>) => o as unknown as FieldOut;
const lk: RuleLookups = {
  fields: [
    field({
      slug: 'priority',
      name: 'Priority',
      type: 'dropdown',
      items: [{ item: 'High' }, { item: 'Low' }],
    }),
    field({ slug: 'due_at', name: 'Due date', type: 'date' }),
  ],
  stages: [
    { id: 's1', name: 'To Do' },
    { id: 's2', name: 'In Review' },
    { id: 's3', name: 'Done' },
  ],
  members: [
    { user_id: 'anna', first_name: 'Anna', last_name: 'Meyer' },
    { user_id: 'eve', first_name: 'Eve', last_name: 'Tester' },
  ],
};
/** A translator that shows the key and the values, so the tests read the structure. */
const t: T = (key, values) =>
  values
    ? `${key}(${Object.entries(values)
        .map(([k, v]) => `${k}=${v}`)
        .join(',')})`
    : key;

describe('explainRule', () => {
  it('names the trigger and every action', () => {
    const s = explainRule(
      {
        trigger_type: 'stage_changed',
        trigger_config: { to: ['s2'] },
        steps: [
          { action_type: 'change_assignee', config: { add: ['anna'] } },
          {
            action_type: 'create_comment',
            config: { content: 'Please review' },
          },
        ],
      },
      lk,
      t,
    );
    expect(s).toContain('explain.stage_to(to=In Review)');
    expect(s).toContain('explain.add_assignee(user=Anna Meyer)');
    expect(s).toContain('explain.create_comment(text=Please review)');
    expect(s.startsWith('explain.sentence')).toBe(true);
  });

  it('explains dates, fields and webhooks, and a rule without actions', () => {
    const date = explainRule(
      {
        trigger_type: 'date_arrives',
        trigger_config: { field: 'due_at', offset_minutes: -1440 },
        steps: [],
      },
      lk,
      t,
    );
    expect(date).toContain('explain.date_before(hours=24,field=Due date)');
    expect(date).toContain('explain.nothing');
    const f = explainRule(
      {
        trigger_type: 'field_updated',
        trigger_config: { field: 'priority', to: 'High' },
        steps: [
          {
            action_type: 'send_webhook',
            config: { url: 'https://hooks.example.test/x?y=1' },
          },
        ],
      },
      lk,
      t,
    );
    expect(f).toContain('explain.field_to(field=Priority,to=High)');
    expect(f).toContain('explain.send_webhook(host=hooks.example.test)');
  });

  it('shortens long comments', () => {
    const s = explainRule(
      {
        trigger_type: 'card_created',
        trigger_config: {},
        steps: [
          {
            action_type: 'create_comment',
            config: { content: 'x'.repeat(100) },
          },
        ],
      },
      lk,
      t,
    );
    expect(s).toContain(`${'x'.repeat(57)}…`);
  });
});

describe('failed runs', () => {
  it("turns the engine's errors into reasons, and keeps what it does not know", () => {
    expect(explainFailure('webhook url must use https', t)).toBe('fail.https');
    expect(explainFailure('webhook target is not a public address', t)).toBe(
      'fail.private',
    );
    expect(explainFailure('create_card needs `list_id`', t)).toBe(
      'fail.needs(what=list_id)',
    );
    expect(explainFailure('automation owner no longer has access', t)).toBe(
      'fail.owner',
    );
    expect(explainFailure('something new', t)).toBeNull();
    expect(explainFailure(null, t)).toBeNull();
  });

  it('summarizes runs by reason', () => {
    const runs = [
      { status: 'completed' },
      { status: 'failed', error: 'webhook url must use https' },
      { status: 'failed', error: 'webhook url must use https' },
      { status: 'failed', error: 'odd' },
    ];
    expect(summarizeRuns(runs, t)).toBe(
      'fail.summary(failed=3,total=4,reasons=2 × fail.https · 1 × odd)',
    );
    expect(summarizeRuns([{ status: 'completed' }], t)).toBe(
      'fail.allFine(total=1)',
    );
    expect(summarizeRuns([], t)).toBe('fail.noRuns');
  });
});

describe('parseRuleText', () => {
  it('drafts the example: created, assign, comment', () => {
    const d = parseRuleText(
      'When an issue is created, assign it to Anna and comment "Welcome, please start here"',
      lk,
    );
    expect(d?.rule).toEqual({
      trigger_type: 'card_created',
      trigger_config: {},
      steps: [
        { action_type: 'change_assignee', config: { add: ['anna'] } },
        {
          action_type: 'create_comment',
          config: { content: 'Welcome, please start here' },
        },
      ],
    });
    expect(d?.unknown).toEqual([]);
  });

  it('reads a status trigger, a status action and a field action', () => {
    const d = parseRuleText(
      'when an issue moves to Done, set priority to Low',
      lk,
    );
    expect(d?.rule.trigger_type).toBe('stage_changed');
    expect(d?.rule.trigger_config).toEqual({ to: ['s3'] });
    expect(d?.rule.steps).toEqual([
      { action_type: 'set_field', config: { field: 'priority', value: 'Low' } },
    ]);
    const m = parseRuleText(
      'when priority changes to High then move it to In Review',
      lk,
    );
    expect(m?.rule.trigger_config).toEqual({ field: 'priority', to: 'High' });
    expect(m?.rule.steps).toEqual([
      { action_type: 'set_stage', config: { stage_id: 's2' } },
    ]);
  });

  it('understands German', () => {
    const d = parseRuleText(
      'Wenn ein Kommentar hinzugefügt wird, dann weise Eve zu',
      lk,
    );
    expect(d?.rule.trigger_type).toBe('comment_created');
    expect(d?.rule.steps).toEqual([
      { action_type: 'change_assignee', config: { add: ['eve'] } },
    ]);
  });

  it('reads a date trigger', () => {
    const d = parseRuleText(
      'when the due date arrives, comment "Due today"',
      lk,
    );
    expect(d?.rule.trigger_type).toBe('date_arrives');
    expect(d?.rule.trigger_config).toEqual({ field: 'due_at' });
  });

  it('reports what it did not understand, and refuses without trigger or action', () => {
    const d = parseRuleText(
      'when an issue is created, assign it to Anna and tidy the garden',
      lk,
    );
    expect(d?.unknown).toEqual(['tidy the garden']);
    expect(d?.rule.steps).toHaveLength(1);
    expect(parseRuleText('assign everything to Anna', lk)).toBeNull();
    expect(parseRuleText('when an issue is created, juggle', lk)).toBeNull();
    expect(
      parseRuleText('when the moon is full, assign it to Anna', lk),
    ).toBeNull();
  });
});
