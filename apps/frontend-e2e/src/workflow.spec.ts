import { expect, test } from '@playwright/test';
import { createIssue, createWorkspaceSpaceList, register } from './helpers';

test('Jira-style board: create issues, drag them across statuses, work in the issue view', async ({ page }) => {
  await register(page);
  await createWorkspaceSpaceList(page);

  // The default view of a project is its board, with Jira's three statuses.
  await expect(page.getByTestId('board')).toBeVisible();
  for (const s of ['To Do', 'In Progress', 'Done']) await expect(page.getByTestId(`stage-${s}`)).toBeVisible();

  // Issues get keys (ACME-1, ACME-2, ...) and land in the first status.
  await createIssue(page, 'First issue');
  const todo = page.getByTestId('stage-To Do');
  await expect(todo.getByTestId('card-First issue')).toContainText('ACME-1');

  // Create from a column: the issue starts in that status.
  await page.getByTestId('add-card-In Progress').click();
  await page.getByTestId('create-issue').getByLabel('Summary *').fill('Second issue');
  await page.getByTestId('create-issue-submit').click();
  await expect(page.getByTestId('stage-In Progress').getByTestId('card-Second issue')).toBeVisible();

  // Drag and drop between columns.
  await todo.getByTestId('card-First issue').dragTo(page.getByTestId('stage-In Progress'));
  await expect(page.getByTestId('stage-In Progress').getByTestId('card-First issue')).toBeVisible();

  // Issue view (modal): summary, status lozenge menu, details panel, comments, child issue.
  await page.getByTestId('card-First issue').click();
  const issue = page.getByTestId('issue-dialog');
  await expect(issue.getByTestId('issue-key')).toHaveText('ACME-1');
  await issue.getByTestId('status-button').click();
  await page.getByRole('menuitem', { name: 'Done' }).click();
  await expect(issue.getByTestId('status-button')).toContainText('Done');

  await issue.getByLabel('Summary').fill('First issue, renamed');
  await issue.getByLabel('Summary').blur();
  await issue.getByLabel('Story points').fill('5');
  await issue.getByLabel('Story points').blur();
  await issue.getByRole('button', { name: /Priority/ }).click();
  await page.getByRole('option', { name: 'High', exact: true }).click();

  await issue.getByTestId('add-comment').click();
  await issue.getByLabel('Comment').fill('Looks good <script>alert(1)</script>');
  await issue.getByTestId('post-comment').click();
  await expect(issue.getByTestId('activities')).toContainText('Looks good <script>alert(1)</script>'); // text, never HTML
  await issue.getByRole('tab', { name: 'History' }).click();
  await expect(issue.getByTestId('activities')).toContainText('story_points');

  await issue.getByTestId('add-child').click();
  await page.getByTestId('create-issue').getByLabel('Summary *').fill('Child task');
  await page.getByTestId('create-issue-submit').click();
  await expect(issue.getByText('Child task')).toBeVisible();
  await expect(issue.getByText('0 of 1 done')).toBeVisible();

  await issue.getByTestId('watch').click();
  await expect(issue.getByTestId('watch')).toContainText('Stop watching');

  // The board reflects everything: renamed, in Done, with points.
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('stage-Done').getByTestId('card-First issue, renamed')).toContainText('5');

  // Other views of the same project from the sidebar.
  await page.getByTestId('nav-view-List').click();
  await expect(page.getByTestId('issue-list')).toContainText('ACME-1');
  await page.getByTestId('nav-view-Calendar').click();
  await expect(page.getByTestId('calendar')).toBeVisible();
  await page.getByTestId('nav-view-Timeline').click();
  await expect(page.getByTestId('gantt').or(page.getByText('No issues have dates yet.'))).toBeVisible();

  // Search by key jumps straight to the issue.
  await page.getByTestId('top-search').fill('ACME-2');
  await page.getByTestId('top-search').press('Enter');
  await expect(page.getByTestId('issue-key')).toHaveText('ACME-2');
  await page.getByTestId('issue-menu').click();
  page.once('dialog', (d) => d.accept());
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await expect(page).toHaveURL(/workspaces\/[0-9a-f-]+$/);
});
