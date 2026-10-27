import { expect, test } from '@playwright/test';
import {
  createIssue,
  createWorkspaceSpaceList,
  register,
  workspaceKey,
} from './helpers';

test('addresses are readable: the workspace key, never an id', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page, { ws: 'Acme', list: 'Backlog' });

  // The project is named by the workspace key and its own address.
  const key = workspaceKey(page);
  await expect(page).toHaveURL(new RegExp(`/projects/${key}/lists/backlog$`));
  expect(page.url()).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);

  await createIssue(page, 'Readable links');

  // The short address, as shared between people.
  await page.goto(`/browse/${key}-1`);
  await expect(page.getByTestId('issue-key')).toHaveText(`${key}-1`);
  await expect(page.getByLabel('Summary')).toHaveValue('Readable links');
  expect(page.url()).toMatch(new RegExp(`/browse/${key}-1$`));

  // The same issue below its workspace, with the path to it on top.
  await page.goto(`/projects/${key}/issue/${key}-1`);
  await expect(page.getByTestId('issue-key')).toHaveText(`${key}-1`);
  await expect(
    page.getByRole('navigation', { name: 'Breadcrumb' }),
  ).toContainText('Backlog');

  // Lower case works too; the workspace and its settings have plain addresses.
  await page.goto(`/browse/${key.toLowerCase()}-1`);
  await expect(page.getByTestId('issue-key')).toHaveText(`${key}-1`);
  await page.goto(`/projects/${key}`);
  await expect(page.getByTestId('workspace-title')).toHaveText('Acme');
  await page.goto(`/projects/${key}/settings/people`);
  await expect(page.getByTestId('tab-people')).toBeVisible();

  // Unknown keys say so, and tell nothing else.
  await page.goto(`/browse/${key}-999`);
  await expect(page.getByTestId('issue-not-found')).toBeVisible();
  await page.goto('/projects/NOPE');
  await expect(page.getByTestId('workspace-not-found')).toBeVisible();
});
