import {
  expect,
  type APIRequestContext,
  type Locator,
  type Page,
} from '@playwright/test';
import { createHmac, randomBytes } from 'node:crypto';

const MAILPIT = process.env.E2E_MAILPIT_API ?? 'http://127.0.0.1:58025';

/** The first message to `to` that contains a link to `path` (e.g. `verify-email`), as a URL. */
export async function mailLink(
  request: APIRequestContext,
  to: string,
  path: string,
): Promise<string> {
  for (let i = 0; i < 60; i++) {
    const found = await request
      .get(`${MAILPIT}/api/v1/search`, { params: { query: `to:${to}` } })
      .then((r) => r.json())
      .catch(() => ({ messages: [] }));
    for (const m of found.messages ?? []) {
      const full = await (
        await request.get(`${MAILPIT}/api/v1/message/${m.ID}`)
      ).json();
      const link = String(full.Text ?? '').match(
        new RegExp(`https?://\\S+${path}\\?token=[\\w-]+`),
      );
      if (link) return link[0];
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`no mail with a ${path} link for ${to}`);
}

const B32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

/** RFC 6238 code (SHA-1, 6 digits, 30 s) for a base32 secret, at `atMs`. */
export function totp(secretBase32: string, atMs = Date.now()): string {
  let bits = '';
  for (const c of secretBase32.replace(/=+$/, ''))
    bits += B32.indexOf(c).toString(2).padStart(5, '0');
  const key = Buffer.from(bits.match(/.{8}/g)!.map((b) => parseInt(b, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(atMs / 30_000)));
  const h = createHmac('sha1', key).update(counter).digest();
  const o = h[19] & 0x0f;
  const n =
    ((h[o] & 0x7f) << 24) | (h[o + 1] << 16) | (h[o + 2] << 8) | h[o + 3];
  return String(n % 1_000_000).padStart(6, '0');
}

/** Credentials are generated per run; nothing secret is hard-coded. */
export function newUser() {
  const id = randomBytes(6).toString('hex');
  return {
    email: `e2e-${id}@example.test`,
    password: randomBytes(18).toString('base64url'),
    first: 'Eve',
    last: `Tester${id.slice(0, 4)}`,
  };
}

/**
 * With e-mail verification on (the default whenever the stack has SMTP) the account only exists
 * once the link in the confirmation mail was opened. Waits for `landing` to be visible.
 */
export async function finishSignUp(
  page: Page,
  email: string,
  landing: Locator,
) {
  await expect(landing.or(page.getByTestId('confirm-sent'))).toBeVisible();
  if (!(await landing.isVisible())) {
    await page.goto(await mailLink(page.request, email, 'verify-email'));
    await expect(landing).toBeVisible();
  }
}

export async function register(page: Page, u = newUser()) {
  await page.goto('/register');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('First name').fill(u.first);
  await page.getByLabel('Last name').fill(u.last);
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(u.password);
  await page.getByRole('button', { name: 'Create account' }).click();
  await finishSignUp(
    page,
    u.email,
    page.getByText('Create your first workspace'),
  );
  return u;
}

/**
 * Are the AI features on? They are while the administrator has set a language model (admin.spec.ts);
 * the search box then offers "Ask" for a sentence. The access token lives only in the page, so the
 * app itself is asked.
 */
export async function aiEnabled(page: Page): Promise<boolean> {
  const search = page.getByTestId('top-search');
  await search.fill('issues created today');
  const on = await page
    .getByTestId('ask-ai')
    .waitFor({ timeout: 5_000 })
    .then(() => true)
    .catch(() => false);
  await search.fill('');
  return on;
}

export async function login(
  page: Page,
  u: { email: string; password: string },
) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(u.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
}

/**
 * Creates a workspace and its first project. The project's space is created on the fly, so the
 * optional `space` name only matters for tests that look for it later.
 */
export async function createWorkspaceSpaceList(
  page: Page,
  names: { ws: string; list: string; space?: string } = {
    ws: 'Acme',
    list: 'Backlog',
  },
  template: 'Kanban (board)' | 'Scrum (backlog and sprints)' = 'Kanban (board)',
) {
  await page.getByLabel('Workspace name').fill(names.ws);
  await page.getByRole('button', { name: 'Create workspace' }).click();
  await expect(page.getByTestId('workspace-title')).toHaveText(names.ws);
  await page.getByTestId('create-first-project').click();
  await createProject(page, names.list, template);
}

/** Fills the "Create project" dialog that is already open. */
export async function createProject(
  page: Page,
  name: string,
  template: 'Kanban (board)' | 'Scrum (backlog and sprints)' = 'Kanban (board)',
) {
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Project name').fill(name);
  if (template !== 'Kanban (board)') {
    await dialog.getByTestId('list-template').click();
    await page.getByRole('option', { name: template }).click();
  }
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(
    page.getByTestId('list-title').or(page.getByTestId('backlog-title')),
  ).toBeVisible();
}

/** Opens the global "Create" dialog, fills the summary and submits. */
export async function createIssue(
  page: Page,
  summary: string,
  opts: { keepOpen?: boolean } = {},
) {
  await page.getByTestId('create-button').click();
  const dialog = page.getByTestId('create-issue');
  await dialog.getByLabel('Summary *').fill(summary);
  await dialog.getByTestId('create-issue-submit').click();
  if (!opts.keepOpen) await expect(dialog).toBeHidden();
}

/**
 * The key of the workspace the page is in (`/projects/ACME/...`). Keys are unique per instance,
 * so the second workspace called "Acme" gets `ACME2`; tests must not assume `ACME`.
 */
export function workspaceKey(page: Page): string {
  const m = /\/projects\/([A-Z][A-Z0-9]*)/.exec(page.url());
  if (!m) throw new Error(`not inside a workspace: ${page.url()}`);
  return m[1];
}
