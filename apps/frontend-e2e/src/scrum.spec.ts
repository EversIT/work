import { expect, test } from '@playwright/test';
import { createIssue, createWorkspaceSpaceList, register } from './helpers';

test('scrum: backlog, sprint planning by drag and drop, start and complete a sprint', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(
    page,
    { ws: 'Acme', space: 'Eng', list: 'Platform' },
    'Scrum (backlog and sprints)',
  );
  await expect(page.getByTestId('side-nav')).toContainText('Scrum project');

  // The board of a scrum project needs a running sprint.
  await expect(page.getByTestId('sprint-banner')).toContainText(
    'There is no active sprint',
  );
  await page.getByTestId('nav-backlog').click();
  await expect(page.getByTestId('backlog-title')).toBeVisible();

  for (const s of ['Login page', 'Password reset', 'Audit log'])
    await createIssue(page, s);
  const backlog = page.getByTestId('rows-backlog');
  for (const s of ['Login page', 'Password reset', 'Audit log'])
    await expect(backlog.getByTestId(`row-${s}`)).toBeVisible();

  await page.getByTestId('create-sprint').click();
  const sprint = page
    .getByTestId('sprint-ACME Sprint 1')
    .or(page.getByRole('region', { name: /Sprint 1/ }));
  await expect(sprint).toBeVisible();

  // Plan: drag two issues into the sprint.
  const target = page.getByRole('region', { name: /Sprint 1/ });
  await backlog.getByTestId('row-Login page').dragTo(target);
  await expect(target.getByTestId('row-Login page')).toBeVisible();
  await page
    .getByTestId('rows-backlog')
    .getByTestId('row-Password reset')
    .dragTo(target);
  await expect(target.getByTestId('row-Password reset')).toBeVisible();
  await expect(
    page.getByTestId('rows-backlog').getByTestId('row-Audit log'),
  ).toBeVisible();

  // Start the sprint (dialog with duration), then the board shows exactly its issues.
  await target.getByRole('button', { name: 'Start sprint' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Start sprint' })
    .click();
  await expect(target.getByTestId('complete-sprint')).toBeVisible();
  await page.getByTestId('nav-view-Board').click();
  await expect(page.getByTestId('sprint-banner')).toContainText('Sprint 1');
  await expect(page.getByTestId('card-Login page')).toBeVisible();
  await expect(page.getByTestId('card-Audit log')).toHaveCount(0);

  // Finish one issue, complete the sprint; the open one goes back to the backlog.
  await page
    .getByTestId('card-Login page')
    .dragTo(page.getByTestId('stage-Done'));
  await expect(
    page.getByTestId('stage-Done').getByTestId('card-Login page'),
  ).toBeVisible();
  await page.getByTestId('complete-sprint').click();
  await expect(page.getByRole('dialog')).toContainText(
    '1 issue is done, 1 is still open',
  );
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Complete sprint' })
    .click();
  await expect(page.getByTestId('sprint-banner')).toContainText(
    'There is no active sprint',
  );

  await page.getByTestId('nav-backlog').click();
  await expect(
    page.getByTestId('rows-backlog').getByTestId('row-Password reset'),
  ).toBeVisible();
  await expect(
    page.getByTestId('rows-backlog').getByTestId('row-Audit log'),
  ).toBeVisible();
  await expect(
    page.getByTestId('rows-backlog').getByTestId('row-Login page'),
  ).toHaveCount(0); // done work stays in the finished sprint
});

test('backlog: issues are put in order by dragging', async ({ page }) => {
  await register(page);
  await createWorkspaceSpaceList(
    page,
    { ws: 'Acme', list: 'Backlog' },
    'Scrum (backlog and sprints)',
  );
  await page.getByTestId('nav-backlog').click();
  await expect(page.getByTestId('backlog-title')).toBeVisible();
  for (const title of ['First', 'Second', 'Third'])
    await createIssue(page, title);

  const rows = page.getByTestId('rows-backlog');
  const titles = async () =>
    (await rows.locator('[data-testid^="row-"]').allTextContents()).map(
      (x) => /(First|Second|Third)/.exec(x)?.[1],
    );
  await expect.poll(titles).toEqual(['First', 'Second', 'Third']);

  // Drop the last one on the upper half of the first row: it goes in front of it.
  await page.getByTestId('row-Third').dragTo(page.getByTestId('row-First'), {
    targetPosition: { x: 40, y: 3 },
  });
  await expect.poll(titles).toEqual(['Third', 'First', 'Second']);

  // The lower half puts it behind the row.
  await page.getByTestId('row-Third').dragTo(page.getByTestId('row-Second'), {
    targetPosition: { x: 40, y: 25 },
  });
  await expect.poll(titles).toEqual(['First', 'Second', 'Third']);

  // The order is kept: it is stored, not just drawn.
  await page.reload();
  await expect.poll(titles).toEqual(['First', 'Second', 'Third']);
});
