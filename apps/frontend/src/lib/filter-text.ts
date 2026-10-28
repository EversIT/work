import type { FieldOut, StageOut } from '../api/generated/model';

/** One condition, in the shape the filter builder edits (see `components/views/filter-builder.tsx`). */
export type FilterRow = { field: string; operator: string; value: string };

export type FilterContext = {
  fields: FieldOut[];
  stages: StageOut[];
  members: { user_id: string; first_name: string; last_name: string }[];
  cardTypes: { id: string; name: string; icon?: string }[];
  /** The signed-in person, for "assigned to me". */
  meId?: string;
  now?: Date;
};

/** What was understood, for showing it back to the person: a kind and a value. */
export type Part = {
  kind:
    'type' | 'status' | 'assignee' | 'created' | 'updated' | 'field' | 'text';
  /** The field's name, for `field`. */
  field?: string;
  value: string;
};

export type ParsedFilter = { rows: FilterRow[]; parts: Part[] };

/** The virtual "fields" the builder offers besides the issue's own: type and the two dates. */
export const TYPE_FIELD = '__type';

const TYPE_WORDS: Record<string, string[]> = {
  bug: ['bug', 'bugs', 'defect', 'defects', 'fehler'],
  story: ['story', 'stories', 'geschichte', 'geschichten'],
  task: ['task', 'tasks', 'aufgabe', 'aufgaben'],
  epic: ['epic', 'epics'],
  subtask: ['subtask', 'subtasks', 'unteraufgabe', 'unteraufgaben'],
};

/** German words for the usual priorities, mapped to the English item names the default field has. */
const ITEM_SYNONYMS: Record<string, string> = {
  höchste: 'highest',
  kritisch: 'highest',
  hoch: 'high',
  hohe: 'high',
  hoher: 'high',
  mittel: 'medium',
  mittlere: 'medium',
  niedrig: 'low',
  niedrige: 'low',
  niedriger: 'low',
  niedrigste: 'lowest',
};

const STOP = new Set(
  (
    'show me find list all the a an of in on with and that are is have has been issues issue tickets ticket items ' +
    'work assigned to for by about from since who what which where my i please give get ' +
    'zeige zeig mir alle die der das den dem und mit von für zu ist sind vorgänge vorgang ' +
    'finde suche über wo was welche bitte gib priorität prioritäten priority'
  ).split(' '),
);

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const word = (s: string) =>
  new RegExp(`(?<![\\p{L}\\p{N}])${esc(s)}(?![\\p{L}\\p{N}])`, 'iu');

/** Takes the first match of `re` out of `rest`, returning whether there was one. */
function take(state: { rest: string }, re: RegExp): RegExpMatchArray | null {
  const m = state.rest.match(re);
  if (m) state.rest = state.rest.replace(re, ' ');
  return m;
}

const isoDay = (d: Date) => d.toISOString().slice(0, 10);
const startOf = (d: Date) =>
  new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
const addDays = (d: Date, n: number) => new Date(d.getTime() + n * 86_400_000);

/** A period in words -> the first day it covers, and the first day after it (`null` = until now). */
function period(
  state: { rest: string },
  now: Date,
): { from: Date; to: Date | null; label: string } | null {
  const today = startOf(now);
  const monday = addDays(today, -((today.getUTCDay() + 6) % 7));
  const n = state.rest.match(
    /(?<![\p{L}\p{N}])(?:(?:in )?the )?(?:last|past|letzten?|vergangenen?)\s+(\d{1,3})\s+(?:days?|tage[n]?)(?![\p{L}\p{N}])/iu,
  );
  if (n) {
    state.rest = state.rest.replace(n[0], ' ');
    return { from: addDays(today, -Number(n[1])), to: null, label: `${n[1]}d` };
  }
  const table: [
    RegExp,
    () => { from: Date; to: Date | null; label: string },
  ][] = [
    [
      /(?<![\p{L}\p{N}])(?:today|heute)(?![\p{L}\p{N}])/iu,
      () => ({ from: today, to: null, label: 'today' }),
    ],
    [
      /(?<![\p{L}\p{N}])(?:yesterday|gestern)(?![\p{L}\p{N}])/iu,
      () => ({ from: addDays(today, -1), to: today, label: 'yesterday' }),
    ],
    [
      /(?<![\p{L}\p{N}])(?:this week|diese woche|dieser woche)(?![\p{L}\p{N}])/iu,
      () => ({ from: monday, to: null, label: 'thisWeek' }),
    ],
    [
      /(?<![\p{L}\p{N}])(?:last week|past week|letzte woche|letzter woche|vergangene woche)(?![\p{L}\p{N}])/iu,
      () => ({ from: addDays(today, -7), to: null, label: '7d' }),
    ],
    [
      /(?<![\p{L}\p{N}])(?:this month|diesen monat|dieser monat)(?![\p{L}\p{N}])/iu,
      () => ({
        from: new Date(
          Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), 1),
        ),
        to: null,
        label: 'thisMonth',
      }),
    ],
    [
      /(?<![\p{L}\p{N}])(?:last month|past month|letzten monat|letzter monat|vergangenen monat)(?![\p{L}\p{N}])/iu,
      () => ({ from: addDays(today, -30), to: null, label: '30d' }),
    ],
  ];
  for (const [re, make] of table) {
    const m = state.rest.match(re);
    if (m) {
      state.rest = state.rest.replace(re, ' ');
      return make();
    }
  }
  return null;
}

/**
 * Turns a sentence ("bugs assigned to me, high priority, created last week", or the German equivalent)
 * into filter rows. It is rules, not a model: it knows the words for issue types, the fields' own
 * choices, statuses, people, "me", "open/done" and periods, in English and German, and treats the
 * rest as words to look for in the title. What it understood is returned too, so the person
 * sees every condition and can change it.
 */
export function parseFilterText(
  text: string,
  ctx: FilterContext,
): ParsedFilter {
  const now = ctx.now ?? new Date();
  const state = { rest: ` ${text.toLowerCase().replace(/[,;.!?]/g, ' ')} ` };
  const rows: FilterRow[] = [];
  const parts: Part[] = [];
  const add = (row: FilterRow, part: Part) => {
    if (
      !rows.some(
        (r) =>
          r.field === row.field &&
          r.operator === row.operator &&
          r.value === row.value,
      )
    ) {
      rows.push(row);
      parts.push(part);
    }
  };

  // Dates: which date (created by default), then the period.
  const wantsUpdated = take(
    state,
    /(?<![\p{L}\p{N}])(?:updated|changed|modified|aktualisiert|geändert)(?![\p{L}\p{N}])/iu,
  );
  take(
    state,
    /(?<![\p{L}\p{N}])(?:created|opened|erstellt|angelegt)(?![\p{L}\p{N}])/iu,
  );
  const p = period(state, now);
  if (p) {
    const field = wantsUpdated ? 'updated_at' : 'created_at';
    add(
      { field, operator: 'gte', value: isoDay(p.from) },
      { kind: wantsUpdated ? 'updated' : 'created', value: p.label },
    );
    if (p.to)
      add(
        { field, operator: 'lt', value: isoDay(p.to) },
        { kind: wantsUpdated ? 'updated' : 'created', value: p.label },
      );
  }

  // People.
  const assignee = ctx.fields.find((f) => f.is_assignee);
  if (assignee) {
    if (
      take(
        state,
        /(?<![\p{L}\p{N}])(?:unassigned|nicht zugewiesen|ohne bearbeiter|ohne zuständige[nr]?)(?![\p{L}\p{N}])/iu,
      )
    ) {
      add(
        { field: assignee.slug, operator: 'isNull', value: '' },
        { kind: 'assignee', value: 'unassigned' },
      );
    }
    if (
      ctx.meId &&
      take(
        state,
        /(?<![\p{L}\p{N}])(?:assigned to me|assigned to myself|my|mine|mir zugewiesen|von mir|meine[nrs]?)(?![\p{L}\p{N}])/iu,
      )
    ) {
      add(
        { field: assignee.slug, operator: 'in', value: ctx.meId },
        { kind: 'assignee', value: 'me' },
      );
    }
    for (const m of ctx.members) {
      const full = `${m.first_name} ${m.last_name}`.toLowerCase();
      const names = [
        full,
        m.first_name.toLowerCase(),
        m.last_name.toLowerCase(),
      ].filter((n) => n.length >= 3);
      const hit = names.find((n) => word(n).test(state.rest));
      if (hit) {
        state.rest = state.rest.replace(word(hit), ' ');
        add(
          { field: assignee.slug, operator: 'in', value: m.user_id },
          { kind: 'assignee', value: `${m.first_name} ${m.last_name}` },
        );
      }
    }
  }

  // Status: a stage by its name, else "open" / "done".
  const done = ctx.stages.filter((s) => s.is_completed);
  for (const s of [...ctx.stages].sort(
    (a, b) => b.name.length - a.name.length,
  )) {
    if (s.name.length >= 3 && word(s.name.toLowerCase()).test(state.rest)) {
      state.rest = state.rest.replace(word(s.name.toLowerCase()), ' ');
      add(
        { field: 'stage', operator: 'in', value: s.id },
        { kind: 'status', value: s.name },
      );
    }
  }
  if (
    take(
      state,
      /(?<![\p{L}\p{N}])(?:open|unresolved|not done|offen[e]?|unerledigt[e]?|nicht erledigt[e]?)(?![\p{L}\p{N}])/iu,
    )
  ) {
    for (const s of done)
      add(
        { field: 'stage', operator: 'nin', value: s.id },
        { kind: 'status', value: 'open' },
      );
  }
  if (
    take(
      state,
      /(?<![\p{L}\p{N}])(?:done|closed|resolved|completed|erledigt[e]?|geschlossen[e]?|abgeschlossen[e]?|fertig[e]?)(?![\p{L}\p{N}])/iu,
    ) &&
    done.length === 1
  ) {
    add(
      { field: 'stage', operator: 'in', value: done[0].id },
      { kind: 'status', value: done[0].name },
    );
  }

  // Issue types, by name or by the usual words for the type's icon.
  for (const t of ctx.cardTypes) {
    const words = [
      t.name.toLowerCase(),
      `${t.name.toLowerCase()}s`,
      ...(TYPE_WORDS[t.icon ?? ''] ?? []),
    ];
    const hit = words.find((w) => word(w).test(state.rest));
    if (hit) {
      state.rest = state.rest.replace(word(hit), ' ');
      add(
        { field: TYPE_FIELD, operator: 'in', value: t.id },
        { kind: 'type', value: t.name },
      );
    }
  }

  // The fields' own choices (priority, labels, ...). German priority words become the English item.
  for (const [de, en] of Object.entries(ITEM_SYNONYMS))
    state.rest = state.rest.replace(word(de), ` ${en} `);
  for (const f of ctx.fields.filter(
    (x) => (x.type === 'dropdown' || x.type === 'label') && !x.is_title,
  )) {
    const items = (
      Array.isArray(f.items) ? (f.items as { item: unknown }[]) : []
    )
      .map((i) => String(i.item))
      .sort((a, b) => b.length - a.length);
    for (const item of items) {
      if (item.length >= 2 && word(item.toLowerCase()).test(state.rest)) {
        state.rest = state.rest.replace(word(item.toLowerCase()), ' ');
        add(
          { field: f.slug, operator: 'in', value: item },
          { kind: 'field', field: f.name, value: item },
        );
      }
    }
  }
  // "high priority": the word "priority" itself says nothing more.
  for (const f of ctx.fields)
    state.rest = state.rest.replace(word(f.name.toLowerCase()), ' ');

  // What is left are words to look for in the title.
  const title = ctx.fields.find((f) => f.is_title);
  const left = (state.rest.match(/[\p{L}\p{N}_-]+/gu) ?? []).filter(
    (w) => w.length >= 3 && !STOP.has(w),
  );
  if (title && left.length > 0) {
    add(
      { field: title.slug, operator: 'like', value: left.join(' ') },
      { kind: 'text', value: left.join(' ') },
    );
  }
  return { rows, parts };
}
