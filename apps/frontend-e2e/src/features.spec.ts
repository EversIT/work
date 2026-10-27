import { expect, test } from '@playwright/test';
import { createIssue, createWorkspaceSpaceList, register } from './helpers';

test('views group issues by status and by a field, and remember the choice', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Alpha');
  await createIssue(page, 'Beta');
  await page.getByTestId('add-card-In Progress').click();
  await page.getByTestId('create-issue').getByLabel('Summary *').fill('Gamma');
  await page.getByTestId('create-issue-submit').click();
  await expect(
    page.getByTestId('stage-In Progress').getByTestId('card-Gamma'),
  ).toBeVisible();

  // A list view next to the board.
  await page.getByTestId('view-menu').click();
  await page.getByRole('menuitem', { name: 'New view' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('View name').fill('Issues');
  await dialog.getByRole('button', { name: 'View type' }).click();
  await page.getByRole('option', { name: 'List', exact: true }).click();
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(page.getByTestId('issue-list')).toBeVisible();
  await expect(page.getByTestId('groups')).toHaveCount(0);

  const groupBy = async (name: string) => {
    await page.getByRole('button', { name: 'View settings' }).click();
    await page.getByTestId('group-by').click();
    await page.getByRole('option', { name, exact: true }).click();
    await page.getByTestId('list-title').click({ force: true }); // closes the popover
  };

  await groupBy('Status');
  await expect(
    page.getByTestId('group-To Do').getByTestId('group-count'),
  ).toHaveText('2');
  await expect(
    page.getByTestId('group-In Progress').getByTestId('group-count'),
  ).toHaveText('1');
  await expect(
    page.getByTestId('group-Done').getByTestId('group-count'),
  ).toHaveText('0'); // empty statuses stay visible
  await expect(page.getByTestId('group-To Do')).toContainText('Alpha');
  await expect(page.getByTestId('group-In Progress')).not.toContainText(
    'Alpha',
  );

  // A group collapses.
  await page
    .getByTestId('group-To Do')
    .getByRole('button', { name: /To Do/ })
    .click();
  await expect(page.getByTestId('group-To Do')).not.toContainText('Alpha');

  // The choice is part of the view: it survives a reload.
  await page.reload();
  await expect(page.getByTestId('group-In Progress')).toBeVisible();

  // Grouping by a field: nothing has a priority yet, so everything is "No value".
  await groupBy('Priority');
  await expect(
    page.getByTestId('group-No value').getByTestId('group-count'),
  ).toHaveText('3');
});

test('CRM layouts: a contact type gets its own fields and a contact header', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  const workspaceUrl = page.url().split('/lists/')[0];
  await page.goto(`${workspaceUrl}/settings`);
  await page.getByTestId('tab-types').click();
  await page.getByTestId('new-card-type').click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill('Contact');
  await dialog.getByRole('button', { name: 'Layout' }).click();
  await page.getByRole('option', { name: 'Contact (CRM)' }).click();
  await dialog.getByRole('button', { name: 'Create' }).click();
  const fields = page.getByTestId('fields-Contact');
  for (const f of ['Job title', 'Email', 'Phone', 'Company'])
    await expect(fields).toContainText(f);

  // Create a contact from the global Create dialog.
  await page.goto(`${workspaceUrl}/lists/backlog`);
  await page.getByTestId('create-button').click();
  const create = page.getByTestId('create-issue');
  await create
    .getByRole('button', { name: /Issue type|Type/ })
    .first()
    .click();
  await page.getByRole('option', { name: 'Contact' }).click();
  await create.getByLabel('Summary *').fill('Ada Lovelace');
  await create.getByTestId('create-issue-submit').click();
  await expect(create).toBeHidden();

  // The contact's own header and its typed fields.
  await page.getByTestId('card-Ada Lovelace').click();
  const issue = page.getByTestId('issue-dialog');
  await issue.getByLabel('Email').fill('ada@example.test');
  await issue.getByLabel('Email').blur();
  await issue.getByLabel('Job title').fill('Mathematician');
  await issue.getByLabel('Job title').blur();
  const header = issue.getByTestId('crm-person');
  await expect(header).toContainText('Ada Lovelace');
  await expect(header).toContainText('Mathematician');
  await expect(
    header.getByRole('link', { name: 'ada@example.test' }),
  ).toHaveAttribute('href', 'mailto:ada@example.test');

  // The company layout brings its own fields as well.
  await page.keyboard.press('Escape');
  await page.goto(`${workspaceUrl}/settings`);
  await page.getByTestId('tab-types').click();
  await page.getByTestId('new-card-type').click();
  await page.getByRole('dialog').getByLabel('Name').fill('Company');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Layout' })
    .click();
  await page.getByRole('option', { name: 'Company (CRM)' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Create' })
    .click();
  await expect(page.getByTestId('fields-Company')).toContainText('Website');
});
