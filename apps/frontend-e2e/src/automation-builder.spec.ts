import { expect, test } from '@playwright/test';
import { createWorkspaceSpaceList, register } from './helpers';

test('rules are built on their own page: drag actions in, re-order them, scope them to a project', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page, { ws: 'Acme', list: 'Backlog' });

  // Automation lives with the project: its settings, its own section.
  await page.getByRole('link', { name: 'Project settings' }).click();
  await page.getByTestId('ptab-automations').click();
  await expect(page).toHaveURL(/\/lists\/backlog\/settings\/automations$/);
  await page.getByTestId('new-automation').first().click();
  await expect(page.getByTestId('automation-builder')).toBeVisible();
  await expect(page).toHaveURL(/\/automations\/new\?list=/);

  await page.getByLabel('Rule name').fill('Welcome and follow up');

  // The flow starts with the trigger and one action. Set that action up.
  const first = page.getByTestId('node-step-0');
  await expect(first).toContainText('Create a comment');
  await page.getByTestId('node-step-0-select').click();
  await page
    .getByTestId('builder-panel')
    .getByLabel('Comment', { exact: true })
    .fill('Welcome aboard');

  // Drag an action from the palette onto the line before it: it lands first.
  await page
    .getByTestId('palette-create_sub_card')
    .dragTo(page.getByTestId('connector-0'));
  await expect(page.getByTestId('node-step-0')).toContainText(
    'Create a sub-task',
  );
  await expect(page.getByTestId('node-step-1')).toContainText(
    'Create a comment',
  );
  await page
    .getByTestId('builder-panel')
    .getByLabel('Issue summary')
    .fill('Follow up');

  // Drag the first action to the end: the order flips.
  await page
    .getByTestId('node-step-0-grip')
    .dragTo(page.getByTestId('connector-2'));
  await expect(page.getByTestId('node-step-0')).toContainText(
    'Create a comment',
  );
  await expect(page.getByTestId('node-step-1')).toContainText(
    'Create a sub-task',
  );

  // The same without a mouse: select an action and move it with the buttons.
  await page.getByTestId('node-step-0-select').click();
  await page.getByRole('button', { name: 'Move down' }).click();
  await expect(page.getByTestId('node-step-0')).toContainText(
    'Create a sub-task',
  );
  await page.getByRole('button', { name: 'Move up' }).click();
  await expect(page.getByTestId('node-step-0')).toContainText(
    'Create a comment',
  );

  await page.getByTestId('builder-save').click();
  await expect(page.getByTestId('builder-dirty')).toHaveCount(0);
  await expect(page).toHaveURL(/\/automations\/[0-9a-f-]{36}\?list=/);

  // Saved: it is in the project's list, in the saved order, and reopens as built.
  await page.getByTestId('builder-back').click();
  await expect(page).toHaveURL(/\/lists\/backlog\/settings\/automations$/);
  const row = page.getByTestId('automation-Welcome and follow up');
  await expect(row).toContainText('Create a comment');
  await expect(row).toContainText('Create a sub-task');
  await row.getByRole('link', { name: 'Welcome and follow up' }).click();
  await expect(page.getByTestId('node-step-0')).toContainText(
    'Create a comment',
  );
  await expect(page.getByTestId('node-step-1')).toContainText(
    'Create a sub-task',
  );
  await expect(page.getByLabel('Rule name')).toHaveValue(
    'Welcome and follow up',
  );

  // The workspace settings list all rules and say where each one applies.
  await page.goto(
    page.url().replace(/\/automations\/.*/, '/settings/automations'),
  );
  await expect(
    page.getByTestId('automation-Welcome and follow up'),
  ).toContainText('Backlog');
});
