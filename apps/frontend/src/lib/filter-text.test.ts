import { describe, expect, it } from 'vitest';
import type { FieldOut, StageOut } from '../api/generated/model';
import { parseFilterText, TYPE_FIELD, type FilterContext } from './filter-text';

const field = (
  o: Partial<FieldOut> & { slug: string; name: string; type: string },
) => o as unknown as FieldOut;
const ctx: FilterContext = {
  now: new Date('2026-10-28T10:00:00Z'), // a Wednesday
  meId: 'me',
  fields: [
    field({ slug: 'title', name: 'Summary', type: 'text', is_title: true }),
    field({
      slug: 'assignee',
      name: 'Assignee',
      type: 'user',
      is_assignee: true,
    }),
    field({
      slug: 'priority',
      name: 'Priority',
      type: 'dropdown',
      items: [
        { item: 'Highest' },
        { item: 'High' },
        { item: 'Medium' },
        { item: 'Low' },
      ],
    }),
    field({
      slug: 'labels',
      name: 'Labels',
      type: 'label',
      items: [{ item: 'backend' }, { item: 'ux' }],
    }),
  ],
  stages: [
    { id: 's1', name: 'To Do', is_completed: false },
    { id: 's2', name: 'In Progress', is_completed: false },
    { id: 's3', name: 'Done', is_completed: true },
  ] as StageOut[],
  members: [
    { user_id: 'me', first_name: 'Eve', last_name: 'Tester' },
    { user_id: 'anna', first_name: 'Anna', last_name: 'Meyer' },
  ],
  cardTypes: [
    { id: 't-bug', name: 'Bug', icon: 'bug' },
    { id: 't-story', name: 'Story', icon: 'story' },
    { id: 't-task', name: 'Task', icon: 'task' },
  ],
};
const rows = (text: string) => parseFilterText(text, ctx).rows;

describe('parseFilterText', () => {
  it('turns the example sentence into three conditions', () => {
    expect(
      rows('bugs assigned to me, high priority, created last week'),
    ).toEqual([
      { field: 'created_at', operator: 'gte', value: '2026-10-21' },
      { field: 'assignee', operator: 'in', value: 'me' },
      { field: TYPE_FIELD, operator: 'in', value: 't-bug' },
      { field: 'priority', operator: 'in', value: 'High' },
    ]);
  });

  it('understands the German equivalent', () => {
    expect(
      rows(
        'Fehler, die mir zugewiesen sind, hohe Priorität, erstellt letzte Woche',
      ),
    ).toEqual([
      { field: 'created_at', operator: 'gte', value: '2026-10-21' },
      { field: 'assignee', operator: 'in', value: 'me' },
      { field: TYPE_FIELD, operator: 'in', value: 't-bug' },
      { field: 'priority', operator: 'in', value: 'High' },
    ]);
  });

  it('keeps "highest" apart from "high", and finds people by name', () => {
    expect(rows('highest priority stories for Anna')).toEqual([
      { field: 'assignee', operator: 'in', value: 'anna' },
      { field: TYPE_FIELD, operator: 'in', value: 't-story' },
      { field: 'priority', operator: 'in', value: 'Highest' },
    ]);
  });

  it('reads status words: open, done, or a stage by name', () => {
    expect(rows('open tasks')).toEqual([
      { field: 'stage', operator: 'nin', value: 's3' },
      { field: TYPE_FIELD, operator: 'in', value: 't-task' },
    ]);
    expect(rows('done')).toEqual([
      { field: 'stage', operator: 'in', value: 's3' },
    ]);
    expect(rows('in progress')).toEqual([
      { field: 'stage', operator: 'in', value: 's2' },
    ]);
  });

  it('reads periods, and "updated" instead of "created"', () => {
    expect(rows('updated today')).toEqual([
      { field: 'updated_at', operator: 'gte', value: '2026-10-28' },
    ]);
    expect(rows('created yesterday')).toEqual([
      { field: 'created_at', operator: 'gte', value: '2026-10-27' },
      { field: 'created_at', operator: 'lt', value: '2026-10-28' },
    ]);
    expect(rows('this week')).toEqual([
      { field: 'created_at', operator: 'gte', value: '2026-10-26' },
    ]);
    expect(rows('created in the last 3 days')).toEqual([
      { field: 'created_at', operator: 'gte', value: '2026-10-25' },
    ]);
  });

  it('turns unassigned into an empty-field condition and labels into choices', () => {
    expect(rows('unassigned backend')).toEqual([
      { field: 'assignee', operator: 'isNull', value: '' },
      { field: 'labels', operator: 'in', value: 'backend' },
    ]);
  });

  it('looks for the remaining words in the title, and says what it understood', () => {
    const p = parseFilterText('show me all bugs about login form', ctx);
    expect(p.rows).toEqual([
      { field: TYPE_FIELD, operator: 'in', value: 't-bug' },
      { field: 'title', operator: 'like', value: 'login form' },
    ]);
    expect(p.parts.map((x) => x.kind)).toEqual(['type', 'text']);
  });

  it('does nothing with nothing', () => {
    expect(rows('')).toEqual([]);
    expect(rows('show me')).toEqual([]);
  });

  it('does not take "me" for a condition when nobody is signed in', () => {
    expect(
      parseFilterText('assigned to me', { ...ctx, meId: undefined }).rows,
    ).toEqual([]);
  });
});
