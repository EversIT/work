import type {
  ActivityOut,
  CardOut,
  CardTypeRow,
  FieldOut,
  ListRow,
  MemberOut,
  NotificationOut,
  ProjectOut,
  SpaceOut,
  SprintOut,
  StageOut,
  WorkspaceOut,
} from '../api/generated/model';

/** Fixed, readable ids so links in the demo stay stable. */
export const uid = (n: number) =>
  `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;

const DAY = 86_400_000;
const now = Date.now();
export const iso = (offsetDays = 0, hour = 9) => {
  const d = new Date(now + offsetDays * DAY);
  d.setUTCHours(hour, 0, 0, 0);
  return d.toISOString();
};
const date = (offsetDays: number) => iso(offsetDays).slice(0, 10);

export const ME = uid(1);

export const people = [
  { id: ME, first: 'Demo', last: 'User', email: 'demo@example.com' },
  { id: uid(2), first: 'Alex', last: 'Rivera', email: 'alex@example.com' },
  { id: uid(3), first: 'Priya', last: 'Nair', email: 'priya@example.com' },
  { id: uid(4), first: 'Jonas', last: 'Weber', email: 'jonas@example.com' },
  { id: uid(5), first: 'Sam', last: 'Okafor', email: 'sam@example.com' },
];

export const PROJECT = uid(10);
export const WORKSPACE = uid(11);

export const project: ProjectOut = {
  id: PROJECT,
  name: 'Acme Product',
  role: 'owner',
  created_at: iso(-60),
};

export const workspace: WorkspaceOut = {
  id: WORKSPACE,
  project_id: PROJECT,
  name: 'Acme',
  key_prefix: 'ACME',
  created_at: iso(-60),
};

export const members: MemberOut[] = people.map((p, i) => ({
  user_id: p.id,
  email: p.email,
  first_name: p.first,
  last_name: p.last,
  role: i === 0 ? 'owner' : i === 1 ? 'admin' : 'member',
  restricted: false,
}));

export const spaces: SpaceOut[] = [
  {
    id: uid(20),
    workspace_id: WORKSPACE,
    name: 'Engineering',
    icon: 'folder',
    icon_color: 'blue',
    access_type: 'public',
    created_at: iso(-60),
  },
  {
    id: uid(21),
    workspace_id: WORKSPACE,
    name: 'Marketing',
    icon: 'folder',
    icon_color: 'green',
    access_type: 'public',
    created_at: iso(-55),
  },
];

const list = (
  n: number,
  space: number,
  name: string,
  slug: string,
  methodology: 'kanban' | 'scrum',
  icon: string,
  color: string,
): ListRow => ({
  id: uid(n),
  workspace_id: WORKSPACE,
  space_id: uid(space),
  name,
  slug,
  methodology,
  type: 'default_list',
  icon,
  icon_color: color,
  access_type: 'public',
  default_card_type_id: uid(30),
  created_at: iso(-58),
});

export const lists: ListRow[] = [
  list(40, 20, 'Platform', 'platform', 'scrum', 'list', 'blue'),
  list(41, 20, 'Mobile app', 'mobile-app', 'kanban', 'list', 'purple'),
  list(
    42,
    21,
    'Launch campaign',
    'launch-campaign',
    'kanban',
    'list',
    'orange',
  ),
];

const STAGES = [
  ['To do', 'grey', false],
  ['In progress', 'blue', false],
  ['In review', 'purple', false],
  ['Done', 'green', true],
] as const;

export const stages: StageOut[] = lists.flatMap((l, li) =>
  STAGES.map(([name, color, done], i) => ({
    id: uid(100 + li * 10 + i),
    list_id: l.id,
    name,
    color,
    is_completed: done,
    sort_order: (i + 1) * 100,
  })),
);
const stage = (listIdx: number, i: number) => uid(100 + listIdx * 10 + i);

export const cardTypes: CardTypeRow[] = (
  [
    ['Task', 'task', 'blue', true],
    ['Epic', 'epic', 'purple', true],
    ['Story', 'story', 'green', true],
    ['Bug', 'bug', 'red', true],
    ['Sub-task', 'subtask', 'blue', false],
  ] as const
).map(([name, icon, color, children], i) => ({
  id: uid(30 + i),
  workspace_id: WORKSPACE,
  name,
  icon,
  color,
  has_children: children,
  layout: 'default',
  title_template: null,
  created_at: iso(-60),
}));
const TYPE = {
  task: uid(30),
  epic: uid(31),
  story: uid(32),
  bug: uid(33),
  sub: uid(34),
};

const field = (
  n: number,
  name: string,
  slug: string,
  type: string,
  icon: string,
  extra: Partial<FieldOut> = {},
): FieldOut => ({
  id: uid(n),
  workspace_id: WORKSPACE,
  card_type_id: null,
  name,
  slug,
  type,
  icon,
  multiple: false,
  required: false,
  is_title: false,
  is_description: false,
  is_assignee: false,
  is_photo: false,
  is_pinned: false,
  created_by_type: 'system',
  ...extra,
});

const LABELS = [
  'api',
  'backend',
  'bug',
  'collaboration',
  'content',
  'crash',
  'design',
  'filters',
  'frontend',
  'ios',
  'notifications',
  'offline',
  'performance',
  'planning',
  'reports',
  'security',
  'website',
];

export const fields: FieldOut[] = [
  field(50, 'Summary', 'title', 'text', 'text', {
    is_title: true,
    required: true,
  }),
  field(51, 'Description', 'description', 'rich', 'text-box', {
    is_description: true,
  }),
  field(52, 'Assignee', 'assignee', 'user', 'account', { is_assignee: true }),
  field(53, 'Reporter', 'reporter', 'user', 'account'),
  field(54, 'Priority', 'priority', 'dropdown', 'priority', {
    // The server stores a JSON array here; the generated type only knows "object".
    items: ['Highest', 'High', 'Medium', 'Low', 'Lowest'].map((item) => ({
      item,
    })) as unknown as FieldOut['items'],
  }),
  field(55, 'Story points', 'story_points', 'number', 'points'),
  field(56, 'Labels', 'labels', 'label', 'tag', {
    multiple: true,
    items: LABELS.map((item) => ({ item })) as unknown as FieldOut['items'],
  }),
  field(57, 'Start date', 'starts_at', 'date', 'calendar-start'),
  field(58, 'Due date', 'due_at', 'date', 'calendar'),
  field(59, 'Original estimate (hours)', 'estimate_hours', 'number', 'clock'),
];

export const sprints: SprintOut[] = [
  {
    id: uid(60),
    list_id: uid(40),
    name: 'Sprint 1',
    goal: 'Ship sign-in and the first dashboard',
    state: 'completed',
    starts_at: iso(-28),
    ends_at: iso(-15),
    completed_at: iso(-15),
    created_at: iso(-30),
    issue_count: 0,
    done_count: 0,
    points: 0,
  },
  {
    id: uid(61),
    list_id: uid(40),
    name: 'Sprint 2',
    goal: 'Reports, notifications and a faster board',
    state: 'active',
    starts_at: iso(-7),
    ends_at: iso(7),
    completed_at: null,
    created_at: iso(-10),
    issue_count: 0,
    done_count: 0,
    points: 0,
  },
  {
    id: uid(62),
    list_id: uid(40),
    name: 'Sprint 3',
    goal: null,
    state: 'future',
    starts_at: null,
    ends_at: null,
    completed_at: null,
    created_at: iso(-3),
    issue_count: 0,
    done_count: 0,
    points: 0,
  },
];

type Seed = {
  list: number; // index into `lists`
  stage: number; // index into STAGES
  sprint?: number; // uid of the sprint
  type: keyof typeof TYPE;
  title: string;
  text?: string;
  who?: number; // index into people
  priority?: string;
  points?: number;
  labels?: string[];
  due?: number;
  parent?: number; // 1-based card number
};

const S1 = 60;
const S2 = 61;
const seeds: Seed[] = [
  // Platform (scrum)
  {
    list: 0,
    stage: 1,
    sprint: S2,
    type: 'epic',
    title: 'Reporting and dashboards',
    text: 'Everything customers need to see how their projects are doing.',
    who: 1,
    priority: 'High',
    labels: ['reports'],
  },
  {
    list: 0,
    stage: 1,
    sprint: S2,
    type: 'story',
    title: 'Burndown chart for the active sprint',
    text: 'Show remaining points per day next to the board, updated live.',
    who: 2,
    priority: 'High',
    points: 5,
    labels: ['reports', 'frontend'],
    due: 5,
    parent: 1,
  },
  {
    list: 0,
    stage: 0,
    sprint: S2,
    type: 'task',
    title: 'Export a project as CSV',
    text: 'One row per issue with every field, UTF-8, comma separated.',
    who: 3,
    priority: 'Medium',
    points: 3,
    labels: ['reports', 'backend'],
    due: 6,
    parent: 1,
  },
  {
    list: 0,
    stage: 2,
    sprint: S2,
    type: 'bug',
    title: 'Board lags with more than 500 cards',
    text: 'Dragging a card takes over a second on large boards. Needs virtualised columns.',
    who: 1,
    priority: 'Highest',
    points: 8,
    labels: ['performance'],
    due: 3,
  },
  {
    list: 0,
    stage: 3,
    sprint: S2,
    type: 'task',
    title: 'Email digest for unread notifications',
    who: 4,
    priority: 'Medium',
    points: 3,
    labels: ['notifications'],
  },
  {
    list: 0,
    stage: 1,
    sprint: S2,
    type: 'story',
    title: 'Mention teammates in comments',
    text: 'Typing @ opens a list of project members; they get a notification.',
    who: 0,
    priority: 'Medium',
    points: 5,
    labels: ['collaboration'],
    due: 4,
  },
  {
    list: 0,
    stage: 0,
    sprint: S2,
    type: 'task',
    title: 'Dark mode for the timeline view',
    who: 2,
    priority: 'Low',
    points: 2,
    labels: ['frontend'],
  },
  {
    list: 0,
    stage: 3,
    sprint: S2,
    type: 'bug',
    title: 'Wrong due date after daylight saving change',
    who: 3,
    priority: 'High',
    points: 2,
    labels: ['bug'],
  },
  {
    list: 0,
    stage: 0,
    type: 'story',
    title: 'Saved filters shared with the whole team',
    text: 'Save a filter once and let everybody in the project use it.',
    priority: 'Medium',
    points: 5,
    labels: ['filters'],
  },
  {
    list: 0,
    stage: 0,
    type: 'task',
    title: 'Keyboard shortcuts cheat sheet',
    priority: 'Low',
    points: 1,
    labels: ['frontend'],
  },
  {
    list: 0,
    stage: 0,
    type: 'epic',
    title: 'Public API v2',
    text: 'Webhooks with retries and signed payloads, and bulk endpoints.',
    who: 1,
    priority: 'Medium',
    labels: ['api'],
  },
  {
    list: 0,
    stage: 0,
    type: 'task',
    title: 'Rate limit headers on every API response',
    priority: 'Low',
    points: 2,
    labels: ['api', 'backend'],
  },
  {
    list: 0,
    stage: 0,
    sprint: 62,
    type: 'story',
    title: 'Time tracking per issue',
    priority: 'Medium',
    points: 8,
    labels: ['planning'],
  },
  {
    list: 0,
    stage: 0,
    sprint: 62,
    type: 'task',
    title: 'Import issues from CSV',
    priority: 'Low',
    points: 5,
    labels: ['backend'],
  },
  {
    list: 0,
    stage: 3,
    sprint: S1,
    type: 'story',
    title: 'Sign in with a passkey',
    who: 2,
    priority: 'High',
    points: 8,
    labels: ['security'],
  },
  {
    list: 0,
    stage: 3,
    sprint: S1,
    type: 'task',
    title: 'First version of the project dashboard',
    who: 0,
    priority: 'Medium',
    points: 5,
  },
  // Mobile app (kanban)
  {
    list: 1,
    stage: 0,
    type: 'story',
    title: 'Offline mode for issue lists',
    text: 'Cache the last opened lists and queue edits until the connection is back.',
    who: 4,
    priority: 'High',
    points: 13,
    labels: ['offline'],
  },
  {
    list: 1,
    stage: 1,
    type: 'task',
    title: 'Push notifications on iOS',
    who: 3,
    priority: 'High',
    points: 5,
    labels: ['ios'],
    due: 2,
  },
  {
    list: 1,
    stage: 1,
    type: 'bug',
    title: 'Crash when opening an issue without a description',
    who: 4,
    priority: 'Highest',
    labels: ['crash'],
    due: 1,
  },
  {
    list: 1,
    stage: 2,
    type: 'task',
    title: 'Biometric unlock',
    who: 2,
    priority: 'Medium',
    points: 3,
    labels: ['security'],
  },
  {
    list: 1,
    stage: 3,
    type: 'task',
    title: 'App icon and launch screen',
    who: 5 - 1,
    priority: 'Low',
    points: 1,
    labels: ['design'],
  },
  // Marketing (kanban)
  {
    list: 2,
    stage: 1,
    type: 'task',
    title: 'Write the launch announcement',
    text: 'Blog post, newsletter and a short video script.',
    who: 2,
    priority: 'High',
    labels: ['content'],
    due: 4,
  },
  {
    list: 2,
    stage: 0,
    type: 'task',
    title: 'Prepare the comparison page',
    who: 3,
    priority: 'Medium',
    labels: ['website'],
    due: 10,
  },
  {
    list: 2,
    stage: 2,
    type: 'task',
    title: 'Customer quotes for the landing page',
    who: 1,
    priority: 'Medium',
    labels: ['content'],
  },
  {
    list: 2,
    stage: 3,
    type: 'task',
    title: 'Choose launch date',
    who: 0,
    priority: 'High',
    labels: ['planning'],
  },
];

const doc = (text: string) => ({
  type: 'doc',
  content: [{ type: 'paragraph', content: [{ type: 'text', text }] }],
});

export function seedCards(): CardOut[] {
  return seeds.map((s, i) => {
    const number = i + 1;
    const l = lists[s.list];
    const data: Record<string, unknown> = {
      title: s.title,
      reporter: people[(i + 1) % people.length].id,
    };
    if (s.text) data.description = doc(s.text);
    if (s.who !== undefined) data.assignee = [people[s.who].id];
    if (s.priority) data.priority = s.priority;
    if (s.points !== undefined) data.story_points = s.points;
    if (s.labels) data.labels = s.labels;
    if (s.due !== undefined) data.due_at = date(s.due);
    return {
      id: uid(1000 + number),
      workspace_id: WORKSPACE,
      card_type_id: TYPE[s.type],
      parent_id: s.parent ? uid(1000 + s.parent) : null,
      number,
      key: `ACME-${number}`,
      data,
      children_count: seeds.filter((x) => x.parent === number).length,
      created_by: people[0].id,
      created_by_type: 'user',
      created_at: iso(-20 + (i % 15)),
      updated_at: iso(-(i % 6)),
      lists: [
        {
          id: uid(2000 + number),
          list_id: l.id,
          list_stage_id: stage(s.list, s.stage),
          sort_order: (number + 1) * 100,
          sprint_id: s.sprint ? uid(s.sprint) : null,
        },
      ],
    };
  });
}

export function seedActivities(): ActivityOut[] {
  const acts: ActivityOut[] = [];
  const comment = (n: number, who: number, text: string, daysAgo: number) =>
    acts.push({
      id: uid(3000 + acts.length),
      card_id: uid(1000 + n),
      type: 'comment',
      content: { content: text, files: [] },
      created_by: people[who].id,
      created_by_type: 'user',
      created_at: iso(-daysAgo, 14),
    });
  comment(
    2,
    1,
    'I would start with the ideal line and add the actual line once the data endpoint exists.',
    3,
  );
  comment(
    2,
    2,
    'Agreed. I will use the same colours as the board so it reads at a glance.',
    2,
  );
  comment(
    4,
    3,
    'Reproduced with a 800 card seed. The cost is in re-rendering every column on drop.',
    4,
  );
  comment(
    4,
    0,
    'Virtualising the columns should fix it. Moving this to review once the PR is up.',
    1,
  );
  comment(
    18,
    4,
    'Only happens on 17.4 and later; the description field is null there.',
    1,
  );
  comment(
    21,
    2,
    'Draft is in the shared folder. Please add the numbers from the beta programme.',
    2,
  );
  return acts;
}

export function seedNotifications(): NotificationOut[] {
  const n = (
    i: number,
    type: string,
    message: string,
    cardNo: number,
    hoursAgo: number,
    read: boolean,
  ): NotificationOut => ({
    id: uid(4000 + i),
    workspace_id: WORKSPACE,
    type,
    message,
    related_resource_type: 'card',
    related_resource_id: uid(1000 + cardNo),
    color: null,
    is_read: read,
    created_at: new Date(now - hoursAgo * 3_600_000).toISOString(),
  });
  return [
    n(1, 'mention', 'Alex Rivera mentioned you in ACME-2', 2, 2, false),
    n(2, 'assigned', 'Priya Nair assigned ACME-6 to you', 6, 5, false),
    n(3, 'comment', 'Jonas Weber commented on ACME-18', 18, 26, false),
    n(4, 'status', 'ACME-16 was moved to Done', 16, 50, true),
    n(5, 'due', 'ACME-4 is due in 3 days', 4, 70, true),
  ];
}
