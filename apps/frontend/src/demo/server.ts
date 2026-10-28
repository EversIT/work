import * as Y from 'yjs';
import type {
  ActivityOut,
  AutomationOut,
  FieldOut,
  CardOut,
  CardTypeOut,
  ListDetail,
  ListRow,
  Me,
  MemberOut,
  SpaceOut,
  SprintOut,
  StageOut,
  ViewOut,
} from '../api/generated/model';
import {
  ME,
  PROJECT,
  WORKSPACE,
  cardTypes,
  fields,
  iso,
  lists as seedLists,
  members,
  people,
  project,
  seedActivities,
  seedCards,
  seedNotifications,
  spaces as seedSpaces,
  sprints as seedSprints,
  stages as seedStages,
  uid,
  workspace,
} from './seed';

type Json = Record<string, unknown>;
type Result = { status: number; data: unknown };
type Ctx = { p: string[]; q: URLSearchParams; b: Json };

const ok = (data: unknown): Result => ({ status: 200, data });
const done = () => ok({ ok: true });
const notFound = (): Result => ({ status: 404, data: { error: 'not found' } });

// ---- the in-memory "database" -------------------------------------------------------------

/** Board, list, calendar and timeline: the views every project starts with. */
const defaultViews = (listId: string): ViewOut[] =>
  (
    [
      ['Board', 'board', { hideChildren: true }],
      ['List', 'list', {}],
      ['Calendar', 'calendar', {}],
      ['Timeline', 'gantt', {}],
    ] as const
  ).map(([name, type, options], i) => ({
    id: crypto.randomUUID(),
    list_id: listId,
    name,
    type,
    options,
    filters: null,
    created_at: iso(-58, 9 + i),
    updated_at: iso(-58, 9 + i),
  }));

const db = {
  me: {
    id: ME,
    email: 'demo@example.com',
    first_name: 'Demo',
    last_name: 'User',
    mfa_enabled: false,
    is_instance_admin: true,
    auth_method: 'session',
  } as Me,
  projects: [project],
  members: members.map((m): MemberOut => ({ ...m })),
  grants: [] as { list_id: string; user_id: string; role: string }[],
  workspaces: [workspace],
  spaces: [...seedSpaces],
  lists: [...seedLists],
  stages: [...seedStages],
  sprints: [...seedSprints],
  views: seedLists.flatMap((l) => defaultViews(l.id)),
  cards: seedCards(),
  activities: seedActivities(),
  links: [
    {
      id: uid(9001),
      source_card_id: uid(1002),
      target_card_id: uid(1003),
      link_type: 'blocks',
    },
    {
      id: uid(9002),
      source_card_id: uid(1001),
      target_card_id: uid(1004),
      link_type: 'relates',
    },
  ] as {
    id: string;
    source_card_id: string;
    target_card_id: string;
    link_type: string;
  }[],
  notifications: seedNotifications(),
  branding: {
    accent: '#0b66e4',
    title: 'work',
    description: 'Demo instance',
    has_favicon: false,
    version: 1,
  },
  automations: [
    {
      id: uid(7000),
      workspace_id: WORKSPACE,
      name: 'Welcome new issues',
      trigger_type: 'card_created',
      trigger_config: {},
      is_enabled: true,
      has_webhook: false,
      created_by: ME,
      created_at: iso(-20),
      updated_at: iso(-20),
      locations: [{ location_type: 'list', location_id: uid(40) }],
      steps: [
        {
          action_type: 'create_comment',
          config: {
            content: 'Thanks for reporting! We will triage this soon.',
          },
        },
        {
          action_type: 'change_assignee',
          config: { add: [uid(2)], remove: [] },
        },
      ],
    },
    {
      id: uid(7001),
      workspace_id: WORKSPACE,
      name: 'Notify on done',
      trigger_type: 'stage_changed',
      trigger_config: {},
      is_enabled: false,
      has_webhook: false,
      created_by: ME,
      created_at: iso(-12),
      updated_at: iso(-12),
      locations: [],
      steps: [
        {
          action_type: 'send_email',
          config: {
            to: uid(2),
            subject: 'An issue is done',
            body: 'Nice work.',
          },
        },
      ],
    },
  ] as AutomationOut[],
  nextCard: 100,
};

const user = (id: string | null | undefined) => {
  const p = people.find((x) => x.id === id);
  return p
    ? { id: p.id, email: p.email, first_name: p.first, last_name: p.last }
    : undefined;
};

const listCards = (listId: string) =>
  db.cards.filter((c) => c.lists.some((l) => l.list_id === listId));

const stageOf = (c: CardOut, listId: string) =>
  db.stages.find(
    (s) => s.id === c.lists.find((l) => l.list_id === listId)?.list_stage_id,
  );

function sprintOut(s: SprintOut): SprintOut {
  const cs = listCards(s.list_id).filter((c) =>
    c.lists.some((l) => l.list_id === s.list_id && l.sprint_id === s.id),
  );
  return {
    ...s,
    issue_count: cs.length,
    done_count: cs.filter((c) => stageOf(c, s.list_id)?.is_completed).length,
    points: cs.reduce((n, c) => n + (Number(c.data.story_points) || 0), 0),
  };
}

function listDetail(l: ListRow): ListDetail {
  return {
    ...l,
    role: 'owner',
    field_ids: fields.map((f) => f.id),
    stages: db.stages
      .filter((s) => s.list_id === l.id)
      .sort((a, b) => a.sort_order - b.sort_order),
    views: db.views.filter((v) => v.list_id === l.id),
  };
}

const slugify = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '') || 'list';

const typeOut = (): CardTypeOut[] =>
  cardTypes.map((t) => ({
    ...t,
    fields: fields.filter(
      (f) => f.card_type_id === null || f.card_type_id === t.id,
    ),
  }));

function createList(spaceId: string, b: Json): ListRow {
  const id = crypto.randomUUID();
  const l: ListRow = {
    id,
    workspace_id: WORKSPACE,
    space_id: spaceId,
    name: String(b.name ?? 'New list'),
    slug: slugify(String(b.name ?? 'list')),
    methodology: String(b.methodology ?? 'kanban'),
    type: String(b.type ?? 'default_list'),
    icon: String(b.icon ?? 'list'),
    icon_color: String(b.icon_color ?? 'blue'),
    access_type: String(b.access_type ?? 'public'),
    default_card_type_id: uid(30),
    created_at: new Date().toISOString(),
  };
  db.lists.push(l);
  db.views.push(...defaultViews(id));
  (
    [
      ['To do', 'grey', false],
      ['In progress', 'blue', false],
      ['Done', 'green', true],
    ] as const
  ).forEach(([name, color, completed], i) =>
    db.stages.push({
      id: crypto.randomUUID(),
      list_id: id,
      name,
      color,
      is_completed: completed,
      sort_order: (i + 1) * 100,
    }),
  );
  if (l.methodology === 'scrum')
    db.sprints.push({
      id: crypto.randomUUID(),
      list_id: id,
      name: 'Sprint 1',
      goal: null,
      state: 'future',
      starts_at: null,
      ends_at: null,
      completed_at: null,
      created_at: new Date().toISOString(),
      issue_count: 0,
      done_count: 0,
      points: 0,
    });
  return l;
}

function createCard(listId: string, b: Json): CardOut {
  const stage =
    db.stages.find((s) => s.id === b.stage_id) ??
    db.stages
      .filter((s) => s.list_id === listId)
      .sort((a, b2) => a.sort_order - b2.sort_order)[0];
  const number = ++db.nextCard;
  const c: CardOut = {
    id: crypto.randomUUID(),
    workspace_id: WORKSPACE,
    card_type_id: String(b.card_type_id ?? uid(30)),
    parent_id: (b.parent_id as string | null | undefined) ?? null,
    number,
    key: `ACME-${number}`,
    data: { ...(b.data as Json) },
    children_count: 0,
    created_by: ME,
    created_by_type: 'user',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    lists: [
      {
        id: crypto.randomUUID(),
        list_id: listId,
        list_stage_id: stage?.id ?? '',
        sort_order: number * 100,
        sprint_id: null,
      },
    ],
  };
  db.cards.push(c);
  return c;
}

// ---- filters: the same tree the real backend compiles to SQL, evaluated in memory ----

const asList = (v: unknown): unknown[] =>
  Array.isArray(v) ? v : v === null || v === undefined || v === '' ? [] : [v];

function fieldValue(c: CardOut, listId: string, field: string): unknown {
  if (field === 'stage') return stageOf(c, listId)?.id;
  const f = field.replace(/^card\./, '');
  if (f.startsWith('data.')) return c.data[f.slice(5)];
  return (c as unknown as Record<string, unknown>)[f];
}

function matchLeaf(c: CardOut, listId: string, leaf: Json): boolean {
  const val = fieldValue(c, listId, String(leaf.field));
  const want = leaf.value;
  const has = (a: unknown, b: unknown) => String(a) === String(b);
  const empty = asList(val).length === 0;
  switch (leaf.operator) {
    case 'isNull':
      return empty;
    case 'isNotNull':
      return !empty;
    case 'eq':
      return asList(val).some((x) => has(x, want));
    case 'ne':
      return !asList(val).some((x) => has(x, want));
    case 'in':
      return asList(val).some((x) => asList(want).some((w) => has(x, w)));
    case 'nin':
      return !asList(val).some((x) => asList(want).some((w) => has(x, w)));
    case 'like':
      return String(val ?? '')
        .toLowerCase()
        .includes(
          String(want ?? '')
            .replace(/%/g, '')
            .toLowerCase(),
        );
    case 'gt':
    case 'gte':
    case 'lt':
    case 'lte': {
      if (val === null || val === undefined) return false;
      const num = typeof want === 'number' && Number.isFinite(Number(val));
      const a = num ? Number(val) : String(val);
      const b = num ? Number(want) : String(want);
      if (leaf.operator === 'gt') return a > b;
      if (leaf.operator === 'gte') return a >= b;
      if (leaf.operator === 'lt') return a < b;
      return a <= b;
    }
    default:
      return true;
  }
}

function matchNode(c: CardOut, listId: string, node: Json): boolean {
  if ('field' in node) return matchLeaf(c, listId, node);
  const and = (node.and as Json[] | undefined) ?? [];
  const or = (node.or as Json[] | undefined) ?? [];
  return (
    and.every((n) => matchNode(c, listId, n)) &&
    (or.length === 0 || or.some((n) => matchNode(c, listId, n)))
  );
}

function searchCards(listId: string, b: Json) {
  let cs = listCards(listId);
  const kw = String(b.keyword ?? '')
    .trim()
    .toLowerCase();
  if (kw)
    cs = cs.filter(
      (c) =>
        c.key.toLowerCase().includes(kw) ||
        String(c.data.title ?? '')
          .toLowerCase()
          .includes(kw),
    );
  if (b.hide_completed)
    cs = cs.filter((c) => !stageOf(c, listId)?.is_completed);
  if (b.hide_children) cs = cs.filter((c) => !c.parent_id);
  if (b.filters) cs = cs.filter((c) => matchNode(c, listId, b.filters as Json));
  const sprint = b.sprint as string | null | undefined;
  const sprintOf = (c: CardOut) =>
    c.lists.find((l) => l.list_id === listId)?.sprint_id ?? null;
  if (sprint === 'backlog') cs = cs.filter((c) => sprintOf(c) === null);
  else if (sprint === 'active') {
    const a = db.sprints.find(
      (s) => s.list_id === listId && s.state === 'active',
    );
    cs = a ? cs.filter((c) => sprintOf(c) === a.id) : [];
  } else if (sprint) cs = cs.filter((c) => sprintOf(c) === sprint);
  const order = (c: CardOut) =>
    c.lists.find((l) => l.list_id === listId)?.sort_order ?? 0;
  const key = String(b.sort_by ?? 'order');
  const dir = b.sort_order === 'desc' ? -1 : 1;
  cs = [...cs].sort((x, y) => {
    if (key === 'order') return order(x) - order(y);
    const a =
      key === 'created_at' || key === 'updated_at' ? x[key] : x.data[key];
    const c =
      key === 'created_at' || key === 'updated_at' ? y[key] : y.data[key];
    return (a === c ? 0 : (a ?? '') > (c ?? '') ? 1 : -1) * dir;
  });
  const limit = Number(b.limit ?? 50);
  const page = Number(b.page ?? 1);
  return {
    cards: cs.slice((page - 1) * limit, page * limit),
    total: cs.length,
  };
}

const card = (id: string) => db.cards.find((c) => c.id === id);
const sprintById = (id: string) => db.sprints.find((s) => s.id === id);

// ---- routes -------------------------------------------------------------------------------

type Handler = (c: Ctx) => Result;
const routes: [string, RegExp, Handler][] = [];
const on = (method: string, path: string, h: Handler) =>
  routes.push([
    method,
    new RegExp(`^${path.replace(/\{[^}]+\}/g, '([^/]+)')}$`),
    h,
  ]);

// session
on('POST', '/auth/login', () =>
  ok({
    access_token: 'demo',
    expires_in: 3600,
    mfa_required: false,
    verification_required: false,
  }),
);
on('POST', '/auth/register', () =>
  ok({
    access_token: 'demo',
    expires_in: 3600,
    mfa_required: false,
    verification_required: false,
  }),
);
on('POST', '/auth/refresh', () =>
  ok({ access_token: 'demo', expires_in: 3600 }),
);
on('POST', '/auth/logout', done);
on('GET', '/me', () => ok(db.me));
on('PATCH', '/me', ({ b }) => {
  if (typeof b.first_name === 'string') db.me.first_name = b.first_name;
  if (typeof b.last_name === 'string') db.me.last_name = b.last_name;
  return ok(user(ME));
});
on('GET', '/system/info', () =>
  ok({
    email_verification_required: false,
    mail_enabled: false,
    mfa_available: true,
    passkeys_available: true,
    registration_enabled: true,
    slack_enabled: false,
    sso_oidc_name: null,
    sso_providers: [],
  }),
);
on('GET', '/branding', () => ok(db.branding));
on('PATCH', '/admin/branding', ({ b }) => {
  Object.assign(db.branding, b, { version: db.branding.version + 1 });
  return ok(db.branding);
});

// account settings
on('GET', '/me/api-tokens', () => ok([]));
on('POST', '/me/api-tokens', ({ b }) =>
  ok({
    id: crypto.randomUUID(),
    name: b.name,
    scope: b.scope,
    token_prefix: 'wrk_demo',
    token: 'wrk_demo_this_is_not_a_real_token',
    expires_at: null,
    last_used_at: null,
    created_at: new Date().toISOString(),
  }),
);
on('GET', '/me/passkeys', () => ok([]));
on('GET', '/me/sso', () => ok([]));
on('GET', '/me/sessions', () =>
  ok([
    {
      id: uid(9000),
      current: true,
      user_agent: typeof navigator === 'undefined' ? null : navigator.userAgent,
      ip: '127.0.0.1',
      created_at: iso(0, 8),
      last_active_at: new Date().toISOString(),
      expires_at: iso(30),
    },
  ]),
);
on('GET', '/notification-preferences', () => ok([]));
on('GET', '/integrations', () => ok({ connected: [], enabled: [] }));
on('GET', '/automations/catalog', () => ok({ actions: [], triggers: [] }));
on('GET', '/workspaces/{id}/automations', () => ok(db.automations));
on('GET', '/automations/{id}', ({ p }) => {
  const a = db.automations.find((x) => x.id === p[0]);
  return a ? ok(a) : notFound();
});
on('POST', '/workspaces/{id}/automations', ({ p, b }) => {
  const a = {
    id: crypto.randomUUID(),
    workspace_id: p[0],
    name: String(b.name ?? 'Rule'),
    trigger_type: String(b.trigger_type ?? 'card_created'),
    trigger_config: (b.trigger_config as Json) ?? {},
    is_enabled: b.is_enabled !== false,
    has_webhook: false,
    created_by: ME,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    locations: (b.locations as AutomationOut['locations']) ?? [],
    steps: (b.steps as AutomationOut['steps']) ?? [],
  } satisfies AutomationOut;
  db.automations.push(a);
  return ok(a);
});
on('PATCH', '/automations/{id}', ({ p, b }) => {
  const a = db.automations.find((x) => x.id === p[0]);
  if (!a) return notFound();
  Object.assign(a, b, { updated_at: new Date().toISOString() });
  return ok(a);
});
on('DELETE', '/automations/{id}', ({ p }) => {
  db.automations = db.automations.filter((a) => a.id !== p[0]);
  return done();
});
on('GET', '/automations/{id}/runs', () => ok([]));

// projects and workspaces
on('GET', '/projects', () => ok(db.projects));
on('POST', '/projects', ({ b }) => {
  const p = {
    id: crypto.randomUUID(),
    name: String(b.name ?? 'New project'),
    role: 'owner',
    created_at: new Date().toISOString(),
  };
  db.projects.push(p);
  return ok(p);
});
on('GET', '/projects/{id}', ({ p }) => {
  const x = db.projects.find((r) => r.id === p[0]);
  return x ? ok(x) : notFound();
});
on('GET', '/projects/{id}/members', () => ok(db.members));
on('PATCH', '/projects/{id}/members/{id}', ({ p, b }) => {
  const m = db.members.find((x) => x.user_id === p[1]);
  if (!m) return notFound();
  if (typeof b.role === 'string') {
    m.role = b.role;
    if (b.role !== 'viewer') m.restricted = false;
  }
  if (typeof b.restricted === 'boolean') m.restricted = b.restricted;
  return done();
});
on('DELETE', '/projects/{id}/members/{id}', ({ p }) => {
  db.members = db.members.filter((x) => x.user_id !== p[1]);
  return done();
});
on('POST', '/projects/{id}/invites', () =>
  ok({ token: 'demo-invite-token', expires_at: iso(7) }),
);
on('GET', '/projects/{id}/workspaces', ({ p }) =>
  ok(db.workspaces.filter((w) => w.project_id === p[0])),
);
on('POST', '/projects/{id}/workspaces', ({ p, b }) => {
  const w = {
    id: crypto.randomUUID(),
    project_id: p[0],
    name: String(b.name ?? 'Workspace'),
    key_prefix: String(b.key_prefix ?? 'WRK').toUpperCase(),
    created_at: new Date().toISOString(),
  };
  db.workspaces.push(w);
  return ok(w);
});
on('GET', '/workspaces/by-key/{key}', ({ p }) => {
  const w = db.workspaces.find(
    (x) => x.key_prefix.toUpperCase() === p[0].toUpperCase(),
  );
  return w ? ok(w) : notFound();
});
on('GET', '/issues/{key}', ({ p }) => {
  const c = db.cards.find((x) => x.key.toUpperCase() === p[0].toUpperCase());
  return c ? ok(c) : notFound();
});
on('GET', '/workspaces/{id}', ({ p }) => {
  const w = db.workspaces.find((r) => r.id === p[0]);
  return w ? ok(w) : notFound();
});
on('GET', '/workspaces/{id}/tree', ({ p }) =>
  ok({
    spaces: db.spaces
      .filter((s) => s.workspace_id === p[0])
      .map((s) => ({
        ...s,
        lists: db.lists
          .filter((l) => l.space_id === s.id)
          .map(
            ({
              workspace_id: _w,
              created_at: _c,
              default_card_type_id: _d,
              ...brief
            }) => brief,
          ),
      })),
  }),
);
on('GET', '/workspaces/{id}/card-types', () => ok(typeOut()));
on('POST', '/workspaces/{id}/card-types', ({ p, b }) => {
  const t = {
    id: crypto.randomUUID(),
    workspace_id: p[0],
    name: String(b.name ?? 'Type'),
    icon: String(b.icon ?? 'task'),
    color: String(b.color ?? 'blue'),
    has_children: Boolean(b.has_children),
    layout: String(b.layout ?? 'default'),
    title_template: null,
    created_at: new Date().toISOString(),
  };
  cardTypes.push(t);
  return ok(t);
});
on('DELETE', '/card-types/{id}', ({ p }) => {
  const i = cardTypes.findIndex((x) => x.id === p[0]);
  if (i >= 0) cardTypes.splice(i, 1);
  return done();
});
on('GET', '/workspaces/{id}/fields', () => ok(fields));
on('PATCH', '/fields/{id}', ({ p, b }) => {
  const f = fields.find((x) => x.id === p[0]);
  if (!f) return notFound();
  Object.assign(f, b);
  return ok(f);
});
on('POST', '/workspaces/{id}/fields', ({ p, b }) => {
  const name = String(b.name ?? 'Field');
  const f: FieldOut = {
    id: crypto.randomUUID(),
    workspace_id: p[0],
    card_type_id: (b.card_type_id as string | null | undefined) ?? null,
    name,
    slug: slugify(name).replace(/-/g, '_'),
    type: String(b.type ?? 'text'),
    icon: 'text',
    multiple: Boolean(b.multiple),
    required: Boolean(b.required),
    is_title: false,
    is_description: false,
    is_assignee: false,
    is_photo: false,
    is_pinned: false,
    created_by_type: 'user',
    items: (b.items as FieldOut['items']) ?? null,
  };
  fields.push(f);
  return ok(f);
});
on('DELETE', '/fields/{id}', ({ p }) => {
  const i = fields.findIndex((x) => x.id === p[0]);
  if (i >= 0) fields.splice(i, 1);
  return done();
});
on('GET', '/workspaces/{id}/activities', () =>
  ok(db.activities.slice(-10).reverse()),
);
on('GET', '/workspaces/{id}/cards/search', ({ q }) => {
  const kw = (q.get('keyword') ?? '').toLowerCase();
  const t = q.get('card_type_id');
  return ok(
    db.cards
      .filter(
        (c) =>
          (!t || c.card_type_id === t) &&
          (c.key.toLowerCase().includes(kw) ||
            String(c.data.title ?? '')
              .toLowerCase()
              .includes(kw)),
      )
      .slice(0, 20),
  );
});
on('GET', '/workspaces/{id}/cards/by-key/{key}', ({ p }) => {
  const c = db.cards.find((x) => x.key.toLowerCase() === p[1].toLowerCase());
  return c ? ok(c) : notFound();
});
on('GET', '/workspaces/{id}/issues/mine', ({ q }) =>
  ok(
    db.cards.filter((c) => {
      const mine = ([] as unknown[]).concat(c.data.assignee ?? []).includes(ME);
      const finished = c.lists.some((l) => stageOf(c, l.list_id)?.is_completed);
      return mine && (q.get('include_done') === 'true' || !finished);
    }),
  ),
);
on('GET', '/workspaces/{id}/lists/by-slug/{slug}', ({ p }) => {
  const l = db.lists.find((x) => x.slug === p[1]);
  return l ? ok(listDetail(l)) : notFound();
});
on('POST', '/workspaces/{id}/spaces', ({ p, b }) => {
  const s: SpaceOut = {
    id: crypto.randomUUID(),
    workspace_id: p[0],
    name: String(b.name ?? 'Space'),
    icon: String(b.icon ?? 'folder'),
    icon_color: String(b.icon_color ?? 'blue'),
    access_type: String(b.access_type ?? 'public'),
    created_at: new Date().toISOString(),
  };
  db.spaces.push(s);
  return ok(s);
});

// spaces and lists
on('GET', '/spaces/{id}', ({ p }) => {
  const s = db.spaces.find((x) => x.id === p[0]);
  return s ? ok(s) : notFound();
});
on('PATCH', '/spaces/{id}', ({ p, b }) => {
  const s = db.spaces.find((x) => x.id === p[0]);
  if (!s) return notFound();
  Object.assign(s, b);
  return ok(s);
});
on('GET', '/spaces/{id}/grants', () => ok([]));
on('POST', '/spaces/{id}/lists', ({ p, b }) =>
  ok(listDetail(createList(p[0], b))),
);
on('GET', '/lists/{id}', ({ p }) => {
  const l = db.lists.find((x) => x.id === p[0]);
  return l ? ok(listDetail(l)) : notFound();
});
on('PATCH', '/lists/{id}', ({ p, b }) => {
  const l = db.lists.find((x) => x.id === p[0]);
  if (!l) return notFound();
  Object.assign(l, b);
  return ok(listDetail(l));
});
on('POST', '/lists/{id}/cards/search', ({ p, b }) => ok(searchCards(p[0], b)));
on('POST', '/lists/{id}/cards', ({ p, b }) => ok(createCard(p[0], b)));
on('GET', '/lists/{id}/sprints', ({ p }) =>
  ok(db.sprints.filter((s) => s.list_id === p[0]).map(sprintOut)),
);
on('POST', '/lists/{id}/sprints', ({ p, b }) => {
  const n = db.sprints.filter((s) => s.list_id === p[0]).length + 1;
  const s: SprintOut = {
    id: crypto.randomUUID(),
    list_id: p[0],
    name: String(b.name ?? `Sprint ${n}`),
    goal: (b.goal as string | null) ?? null,
    state: 'future',
    starts_at: null,
    ends_at: null,
    completed_at: null,
    created_at: new Date().toISOString(),
    issue_count: 0,
    done_count: 0,
    points: 0,
  };
  db.sprints.push(s);
  return ok(sprintOut(s));
});
on('GET', '/lists/{id}/views', ({ p }) =>
  ok(db.views.filter((v) => v.list_id === p[0])),
);
on('POST', '/lists/{id}/views', ({ p, b }) => {
  const v: ViewOut = {
    id: crypto.randomUUID(),
    list_id: p[0],
    name: String(b.name ?? 'View'),
    type: String(b.type ?? 'board'),
    filters: (b.filters as Json | null) ?? null,
    options: (b.options as Json | null) ?? {},
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
  db.views.push(v);
  return ok(v);
});
on('GET', '/lists/{id}/groups', () => ok([]));
on('GET', '/lists/{id}/grants', ({ p }) =>
  ok(db.grants.filter((g) => g.list_id === p[0])),
);
on('PUT', '/lists/{id}/grants/{id}', ({ p, b }) => {
  const g = db.grants.find((x) => x.list_id === p[0] && x.user_id === p[1]);
  if (g) g.role = String(b.role);
  else db.grants.push({ list_id: p[0], user_id: p[1], role: String(b.role) });
  return done();
});
on('DELETE', '/lists/{id}/grants/{id}', ({ p }) => {
  db.grants = db.grants.filter(
    (g) => !(g.list_id === p[0] && g.user_id === p[1]),
  );
  return done();
});
on('POST', '/lists/{id}/invites', () =>
  ok({ token: 'demo-invite-token', expires_at: iso(3) }),
);
on('DELETE', '/lists/{id}', ({ p }) => {
  db.lists = db.lists.filter((l) => l.id !== p[0]);
  return done();
});
on('POST', '/lists/{id}/stages', ({ p, b }) => {
  const max = Math.max(
    0,
    ...db.stages.filter((s) => s.list_id === p[0]).map((s) => s.sort_order),
  );
  const s: StageOut = {
    id: crypto.randomUUID(),
    list_id: p[0],
    name: String(b.name ?? 'Stage'),
    color: String(b.color ?? 'grey'),
    is_completed: Boolean(b.is_completed),
    sort_order: max + 100,
  };
  db.stages.push(s);
  return ok(s);
});
on('PUT', '/lists/{id}/stages/order', ({ b }) => {
  (b.stage_ids as string[] | undefined)?.forEach((id, i) => {
    const s = db.stages.find((x) => x.id === id);
    if (s) s.sort_order = (i + 1) * 100;
  });
  return done();
});
on('PATCH', '/stages/{id}', ({ p, b }) => {
  const s = db.stages.find((x) => x.id === p[0]);
  if (!s) return notFound();
  Object.assign(s, b);
  return ok(s);
});
on('DELETE', '/stages/{id}', ({ p }) => {
  db.stages = db.stages.filter((s) => s.id !== p[0]);
  return done();
});

// sprints
on('PATCH', '/sprints/{id}', ({ p, b }) => {
  const s = sprintById(p[0]);
  if (!s) return notFound();
  Object.assign(s, b);
  return ok(sprintOut(s));
});
on('POST', '/sprints/{id}/start', ({ p, b }) => {
  const s = sprintById(p[0]);
  if (!s) return notFound();
  s.state = 'active';
  s.starts_at = (b.starts_at as string | undefined) ?? iso(0);
  s.ends_at = (b.ends_at as string | undefined) ?? iso(14);
  return ok(sprintOut(s));
});
on('POST', '/sprints/{id}/complete', ({ p, b }) => {
  const s = sprintById(p[0]);
  if (!s) return notFound();
  const target = (b.move_to as string | null | undefined) ?? null;
  for (const c of db.cards)
    for (const l of c.lists)
      if (l.sprint_id === s.id && !stageOf(c, l.list_id)?.is_completed)
        l.sprint_id = target;
  s.state = 'completed';
  s.completed_at = new Date().toISOString();
  return ok(sprintOut(s));
});
on('DELETE', '/sprints/{id}', ({ p }) => {
  db.sprints = db.sprints.filter((s) => s.id !== p[0]);
  return done();
});

// cards
on('GET', '/cards/{id}', ({ p }) => {
  const c = card(p[0]);
  return c ? ok(c) : notFound();
});
on('PATCH', '/cards/{id}', ({ p, b }) => {
  const c = card(p[0]);
  if (!c) return notFound();
  if (b.data) c.data = { ...c.data, ...(b.data as Json) };
  if ('parent_id' in b) c.parent_id = (b.parent_id as string | null) ?? null;
  c.updated_at = new Date().toISOString();
  return ok(c);
});
on('DELETE', '/cards/{id}', ({ p }) => {
  db.cards = db.cards.filter((c) => c.id !== p[0]);
  return done();
});
on('GET', '/cards/{id}/children', ({ p }) =>
  ok(db.cards.filter((c) => c.parent_id === p[0])),
);
const LINK_TYPES = ['blocks', 'duplicates', 'relates', 'clones'];
const linkOut = (l: (typeof db.links)[number], id: string) => {
  const outward = l.source_card_id === id;
  const other = card(outward ? l.target_card_id : l.source_card_id);
  return (
    other && {
      id: l.id,
      link_type: l.link_type,
      direction: outward || l.link_type === 'relates' ? 'outward' : 'inward',
      card: other,
    }
  );
};
on('GET', '/cards/{id}/links', ({ p }) =>
  ok(
    db.links
      .filter((l) => l.source_card_id === p[0] || l.target_card_id === p[0])
      .map((l) => linkOut(l, p[0]))
      .filter(Boolean),
  ),
);
on('POST', '/cards/{id}/links', ({ p, b }) => {
  const type = String(b.link_type);
  const target = String(b.target_id);
  const bad = (error: string): Result => ({ status: 400, data: { error } });
  if (!LINK_TYPES.includes(type)) return bad('unknown link type');
  if (target === p[0]) return bad('an issue cannot be linked to itself');
  if (!card(p[0]) || !card(target)) return notFound();
  const inward = b.direction === 'inward';
  const [source, to] = inward ? [target, p[0]] : [p[0], target];
  const taken = db.links.some(
    (l) =>
      l.link_type === type &&
      ((l.source_card_id === source && l.target_card_id === to) ||
        (l.source_card_id === to && l.target_card_id === source)),
  );
  if (taken) return { status: 409, data: { error: 'conflict' } };
  const l = {
    id: crypto.randomUUID(),
    source_card_id: source,
    target_card_id: to,
    link_type: type,
  };
  db.links.push(l);
  return ok(linkOut(l, p[0]));
});
on('DELETE', '/cards/{id}/links/{linkId}', ({ p }) => {
  const before = db.links.length;
  db.links = db.links.filter(
    (l) =>
      !(
        l.id === p[1] &&
        (l.source_card_id === p[0] || l.target_card_id === p[0])
      ),
  );
  return db.links.length === before ? notFound() : done();
});
on('GET', '/cards/{id}/activities', ({ p }) =>
  ok(
    db.activities
      .filter((a) => a.card_id === p[0])
      .sort((a, b2) => (a.created_at < b2.created_at ? 1 : -1)),
  ),
);
on('POST', '/cards/{id}/activities', ({ p, b }) => {
  const a: ActivityOut = {
    id: crypto.randomUUID(),
    card_id: p[0],
    type: String(b.type ?? 'comment'),
    content: (b.content as Json) ?? {},
    created_by: ME,
    created_by_type: 'user',
    created_at: new Date().toISOString(),
  };
  db.activities.push(a);
  return ok(a);
});
on('PATCH', '/activities/{id}', ({ p, b }) => {
  const a = db.activities.find((x) => x.id === p[0]);
  if (!a) return notFound();
  if (b.content) a.content = b.content as Json;
  return ok(a);
});
on('DELETE', '/activities/{id}', ({ p }) => {
  db.activities = db.activities.filter((a) => a.id !== p[0]);
  return done();
});
on('PUT', '/cards/{id}/lists/{id}', ({ p, b }) => {
  const c = card(p[0]);
  if (!c) return notFound();
  const link = c.lists.find((l) => l.list_id === p[1]);
  if (link) {
    link.list_stage_id = String(b.stage_id);
    if (typeof b.order === 'number') link.sort_order = b.order;
  } else {
    c.lists.push({
      id: crypto.randomUUID(),
      list_id: p[1],
      list_stage_id: String(b.stage_id),
      sort_order: typeof b.order === 'number' ? b.order : 100,
      sprint_id: null,
    });
  }
  return ok(c);
});
on('DELETE', '/cards/{id}/lists/{id}', ({ p }) => {
  const c = card(p[0]);
  if (c) c.lists = c.lists.filter((l) => l.list_id !== p[1]);
  return done();
});
on('PUT', '/cards/{id}/sprint', ({ p, b }) => {
  const c = card(p[0]);
  const l = c?.lists.find((x) => x.list_id === b.list_id);
  if (!c || !l) return notFound();
  l.sprint_id = (b.sprint_id as string | null | undefined) ?? null;
  return ok(c);
});
// Live editing: the server holds one shared document per field. Here there is nobody to share it
// with, but the editor still starts from it and hands its edits to it.
const b64 = (bytes: Uint8Array) => {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000)
    s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};
const unb64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const collabDocs = new Map<string, Y.Doc>();
on('POST', '/cards/{id}/collab/{id}/sync', ({ p, b }) => {
  const key = `${p[0]}/${p[1]}`;
  let doc = collabDocs.get(key);
  if (!doc && typeof b.seed === 'string') {
    doc = new Y.Doc();
    Y.applyUpdate(doc, unb64(b.seed));
    collabDocs.set(key, doc);
  }
  if (!doc) return ok({ empty: true, state_vector: '', update: '' });
  if (typeof b.update === 'string' && b.update)
    Y.applyUpdate(doc, unb64(b.update));
  const known =
    typeof b.state_vector === 'string' && b.state_vector
      ? unb64(b.state_vector)
      : undefined;
  return ok({
    empty: false,
    state_vector: b64(Y.encodeStateVector(doc)),
    update: b64(Y.encodeStateAsUpdate(doc, known)),
  });
});
on('GET', '/watchers', () => ok([]));
on('GET', '/watchers/is-watching', () => ok({ watching: false }));

// notifications
on('GET', '/notifications', ({ q }) => {
  const read = q.get('is_read');
  return ok(
    db.notifications
      .filter((n) => read === null || n.is_read === (read === 'true'))
      .sort((a, b2) => (a.created_at < b2.created_at ? 1 : -1)),
  );
});
on('POST', '/notifications/read-all', () => {
  db.notifications.forEach((n) => (n.is_read = true));
  return done();
});
on('POST', '/notifications/{id}/read', ({ p }) => {
  const n = db.notifications.find((x) => x.id === p[0]);
  if (n) n.is_read = true;
  return done();
});

// administration
on('GET', '/admin/overview', () =>
  ok({
    config: {
      email_enabled: false,
      email_verification_required: false,
      mfa_available: true,
      registration_enabled: true,
      scheduler: true,
      shared_rate_limit: false,
      sso_providers: [],
      storage: 'local',
      version: 'demo',
    },
    counts: {
      active_sessions: 1,
      admins: 1,
      api_tokens: 0,
      automations_enabled: 0,
      cards: db.cards.length,
      comments: db.activities.length,
      file_bytes: 0,
      files: 0,
      lists: db.lists.length,
      projects: db.projects.length,
      users: people.length,
      users_disabled: 0,
      users_with_mfa: 0,
      users_with_passkeys: 0,
      workspaces: db.workspaces.length,
    },
  }),
);
on('GET', '/admin/users', () =>
  ok({
    total: people.length,
    users: people.map((p, i) => ({
      id: p.id,
      email: p.email,
      first_name: p.first,
      last_name: p.last,
      created_at: iso(-60 + i),
      last_active_at: iso(-i),
      disabled: false,
      locked: false,
      has_password: true,
      is_instance_admin: i === 0,
      mfa_enabled: false,
      passkeys: 0,
      projects: 1,
      sso: [],
    })),
  }),
);
on('GET', '/admin/projects', () =>
  ok([
    {
      id: PROJECT,
      name: project.name,
      owner_email: 'demo@example.com',
      created_at: project.created_at,
      members: members.length,
      workspaces: db.workspaces.length,
      cards: db.cards.length,
      file_bytes: 0,
    },
  ]),
);
on('GET', '/admin/audit', () =>
  ok([
    {
      id: 3,
      at: iso(-1, 10),
      action: 'user.sign_in',
      actor_email: 'demo@example.com',
      target: null,
      detail: {},
    },
    {
      id: 2,
      at: iso(-3, 16),
      action: 'branding.update',
      actor_email: 'demo@example.com',
      target: null,
      detail: {},
    },
    {
      id: 1,
      at: iso(-60, 9),
      action: 'user.create',
      actor_email: null,
      target: 'demo@example.com',
      detail: {},
    },
  ]),
);

// ---- entry point --------------------------------------------------------------------------

/** Answers an API request from the in-memory data. Unknown writes succeed without effect. */
export function demoRequest(url: string, options: RequestInit = {}): Result {
  const [rawPath, query = ''] = url.split('?');
  const path = rawPath.replace(/^\/api\/v1/, '');
  const method = (options.method ?? 'GET').toUpperCase();
  let b: Json = {};
  if (typeof options.body === 'string') {
    try {
      b = JSON.parse(options.body) as Json;
    } catch {
      b = {};
    }
  }
  for (const [m, re, handler] of routes) {
    if (m !== method) continue;
    const hit = re.exec(path);
    if (hit)
      return handler({ p: hit.slice(1), q: new URLSearchParams(query), b });
  }
  if (method === 'GET') return notFound();
  return done();
}
