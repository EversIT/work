import { expect, test } from '@playwright/test';
import {
  aiEnabled,
  createIssue,
  createWorkspaceSpaceList,
  register,
} from './helpers';

// The AI features need a language model: admin.spec.ts enters the stand-in of fake-model.mjs in the
// administration. It translates the sentence below the way a real model would.
const SENTENCE = 'Login fails after the update.';
const GERMAN = 'Die Anmeldung schlägt nach dem Update fehl.';

test('the assistant helps where people write: create dialog, comment box, editor', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  test.skip(
    !(await aiEnabled(page)),
    'the AI features are off: admin.spec.ts sets the language model first',
  );
  await createIssue(page, 'Login fails after the update');

  // Create dialog: similar issues show up while the summary is typed.
  await page.getByTestId('create-button').click();
  const create = page.getByTestId('create-issue');
  await create
    .getByLabel('Summary *')
    .fill('Login fails after the update again');
  await expect(create.getByTestId('create-similar')).toContainText(
    'Login fails after the update',
  );
  await create.getByRole('button', { name: 'Cancel' }).click();
  await expect(create).toBeHidden();

  // Comment box: quick replies fill the editor, the person edits and sends.
  await page.getByTestId('card-Login fails after the update').click();
  const issue = page.getByTestId('issue-dialog');
  await issue.getByTestId('add-comment').click();
  const editor = issue.getByTestId('comment-editor');
  const box = editor.getByRole('textbox', { name: 'Comment' });
  await issue.getByRole('button', { name: 'Thanks, that helps!' }).click();
  await expect(box).toContainText('Thanks, that helps!');

  // Editor: translate the selection; it is shown next to the original and only replaced on Accept.
  await box.click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type(SENTENCE);
  await page.keyboard.press('ControlOrMeta+a');
  await editor.getByTestId('ai-writing').selectOption('translate:de');
  const proposal = page.getByTestId('ai-writing-dialog');
  await expect(proposal).toContainText(GERMAN);
  await expect(box).toContainText(SENTENCE); // nothing replaced yet
  await proposal.getByRole('button', { name: 'Discard' }).click();
  await expect(box).toContainText(SENTENCE);

  await box.click();
  await page.keyboard.press('ControlOrMeta+a');
  await editor.getByTestId('ai-writing').selectOption('translate:de');
  await page.getByTestId('ai-accept').click();
  await expect(box).toContainText(GERMAN);
  await expect(box).not.toContainText(SENTENCE);

  // The issue's own assistant menu translates the description the same way, into the same two languages.
  await expect(issue.getByTestId('ai-menu')).toBeVisible();
});
