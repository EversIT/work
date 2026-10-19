import { expect, type Page } from '@playwright/test';
import { randomBytes } from 'node:crypto';

/** Credentials are generated per run; nothing secret is hard-coded. */
export function newUser() {
  const id = randomBytes(6).toString('hex');
  return { email: `e2e-${id}@example.test`, password: randomBytes(18).toString('base64url'), first: 'Eve', last: `Tester${id.slice(0, 4)}` };
}

export async function register(page: Page, u = newUser()) {
  await page.goto('/register');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('First name').fill(u.first);
  await page.getByLabel('Last name').fill(u.last);
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(u.password);
  await page.getByRole('button', { name: 'Create account' }).click();
  await expect(page.getByText('Create your first workspace')).toBeVisible();
  return u;
}

export async function login(page: Page, u: { email: string; password: string }) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(u.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
}

/** Creates a workspace, a space and a project (list). `template` is the project's methodology. */
export async function createWorkspaceSpaceList(
  page: Page,
  names = { ws: 'Acme', space: 'Team', list: 'Backlog' },
  template: 'Kanban (board)' | 'Scrum (backlog and sprints)' = 'Kanban (board)',
) {
  await page.getByLabel('Workspace name').fill(names.ws);
  await page.getByRole('button', { name: 'Create workspace' }).click();
  await expect(page.getByTestId('workspace-title')).toHaveText(names.ws);
  await page.getByTestId('new-space').click();
  await page.getByRole('dialog').getByLabel('Name').fill(names.space);
  await page.getByRole('dialog').getByRole('button', { name: 'Create' }).click();
  await expect(page.getByTestId('side-nav').getByText(names.space, { exact: true })).toBeVisible();
  await page.getByTestId(`new-list-${names.space}`).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill(names.list);
  if (template !== 'Kanban (board)') {
    await dialog.getByTestId('list-template').click();
    await page.getByRole('option', { name: template }).click();
  }
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(page.getByTestId('list-title').or(page.getByTestId('backlog-title'))).toBeVisible();
}

/** Opens the global "Create" dialog, fills the summary and submits. */
export async function createIssue(page: Page, summary: string, opts: { keepOpen?: boolean } = {}) {
  await page.getByTestId('create-button').click();
  const dialog = page.getByTestId('create-issue');
  await dialog.getByLabel('Summary *').fill(summary);
  await dialog.getByTestId('create-issue-submit').click();
  if (!opts.keepOpen) await expect(dialog).toBeHidden();
}
