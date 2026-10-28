import { expect, test } from '@playwright/test';
import {
  aiEnabled,
  createIssue,
  createWorkspaceSpaceList,
  register,
  workspaceKey,
} from './helpers';

test('describe a filter, ask in the search box, summarize the board, catch up on the home page', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  test.skip(
    !(await aiEnabled(page)),
    'the AI features are off: admin.spec.ts sets the language model first',
  );
  await createIssue(page, 'Login fails after the update');

  // "Describe the filter": a sentence becomes conditions, and they really filter.
  await page.getByTestId('filters').click();
  const builder = page.getByTestId('filter-builder');
  await builder.getByLabel('Describe the filter').fill('created today');
  await builder.getByTestId('describe-filter').click();
  await expect(page.getByTestId('filters')).toContainText('(1)');
  await expect(
    page.getByTestId('card-Login fails after the update'),
  ).toBeVisible();
  await builder.getByLabel('Describe the filter').fill('created yesterday');
  await builder.getByTestId('describe-filter').click();
  await expect(page.getByTestId('filters')).toContainText('(2)');
  await expect(
    page.getByTestId('card-Login fails after the update'),
  ).toBeHidden();
  await builder.getByLabel('Describe the filter').fill('quantum entanglement');
  await builder.getByTestId('describe-filter').click();
  await expect(page.getByTestId('filters')).toContainText('(1)'); // "title contains"
  await page.keyboard.press('Escape');

  // The board summary counts the issues of the board.
  await page.getByTestId('summarize-board').click();
  const summary = page.getByTestId('board-summary');
  await expect(summary).toContainText('1 issue on the board');
  await expect(summary).toContainText('Nothing looks stuck');
  await summary.getByRole('button', { name: 'Close' }).last().click();

  // "Ask" in the top search: a sentence, run as conditions, with what was understood shown.
  await page.getByTestId('top-search').fill('issues created today');
  await page.getByTestId('ask-ai').click();
  const answer = page.getByTestId('ask-answer');
  await expect(answer).toContainText('Understood as');
  await expect(answer).toContainText('Login fails after the update');
});

test('the project settings suggest workflows and fields, and rules can be drafted and explained', async ({
  page,
}) => {
  test.setTimeout(120_000); // a long walk through three settings pages
  await register(page);
  await createWorkspaceSpaceList(page);
  test.skip(
    !(await aiEnabled(page)),
    'the AI features are off: admin.spec.ts sets the language model first',
  );
  const key = workspaceKey(page);
  await page.getByRole('link', { name: 'Project settings' }).click();

  // Workflow: describe the team, add the proposed statuses.
  await page.getByTestId('ptab-workflow').click();
  await page
    .getByLabel('Describe your team or project')
    .fill('support team with triage and customer feedback');
  await page.getByTestId('suggest-statuses-go').click();
  const proposals = page.getByTestId('status-proposals');
  await expect(proposals).toContainText('Customer support');
  await expect(proposals).toContainText('Triage');
  await proposals.getByRole('button', { name: 'Add statuses' }).first().click();
  // The names sit in text boxes, so read their values.
  const names = () =>
    page
      .locator('[data-testid="statuses"] input')
      .evaluateAll((els) => els.map((e) => (e as HTMLInputElement).value));
  await expect
    .poll(names)
    .toEqual(
      expect.arrayContaining([
        'New',
        'Triage',
        'Waiting for customer',
        'Resolved',
      ]),
    );

  // Fields: a bug tracker for a mobile app; tick, create, and the fields are on.
  await page.getByTestId('ptab-fields').click();
  await expect(page.getByTestId('project-fields')).toBeVisible(); // the old tab is gone
  await page
    .getByLabel('Describe your team or project')
    .fill('bug tracker for a mobile app');
  await page.getByTestId('suggest-fields-go').click();
  const fields = page.getByTestId('field-proposals');
  await expect(fields).toContainText('Severity');
  await expect(fields).toContainText('App version');
  await fields
    .getByRole('button', { name: 'Create and switch on' })
    .first()
    .click();
  await expect(page.getByTestId('project-fields')).toContainText('Severity');

  // Rules: a sentence becomes a draft in the builder, and the rule is explained in one sentence.
  const sentence = 'when an issue is created, comment "Welcome aboard"';
  await page.goto(
    `/projects/${key}/automations/new?describe=${encodeURIComponent(sentence)}`,
  );
  await expect(page.getByTestId('rule-understood')).toContainText(
    'Understood as',
  );
  await page.getByTestId('explain-rule').click();
  await expect(page.getByTestId('rule-explained')).toContainText(
    'When an issue is created, comment “Welcome aboard”.',
  );
});

test('the home page catches you up, and the backlog helpers answer on an empty history', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(
    page,
    { ws: 'Acme', list: 'Sprints' },
    'Scrum (backlog and sprints)',
  );
  test.skip(
    !(await aiEnabled(page)),
    'the AI features are off: admin.spec.ts sets the language model first',
  );
  await createIssue(page, 'Plan the first sprint');

  await page.goto(`/projects/${workspaceKey(page)}/lists/sprints/backlog`);
  await expect(page.getByTestId('backlog-title')).toBeVisible();

  // Backlog: nothing finished yet, so there is no velocity and no estimate; the answers say so.
  await page.getByTestId('suggest-sprint').click();
  await expect(page.getByTestId('sprint-plan')).toContainText(
    'no finished sprint yet',
  );
  await page
    .getByTestId('sprint-plan')
    .getByRole('button', { name: 'Close' })
    .last()
    .click();
  await page.getByTestId('estimate-backlog').click();
  await expect(page.getByTestId('estimates')).toContainText(
    'Nothing to estimate',
  );
  await page
    .getByTestId('estimates')
    .getByRole('button', { name: 'Close' })
    .last()
    .click();
  await page.getByTestId('group-backlog').click();
  await expect(page.getByTestId('topic-groups')).toContainText(
    'No topics found',
  );
  await page
    .getByTestId('topic-groups')
    .getByRole('button', { name: 'Close' })
    .last()
    .click();

  // Home: a "for you" line from the issues.
  await page.goto(`/projects/${workspaceKey(page)}`);
  await expect(page.getByTestId('catch-me-up')).toBeVisible();
});
