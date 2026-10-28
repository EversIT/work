import { expect, test } from '@playwright/test';
import {
  createIssue,
  createWorkspaceSpaceList,
  register,
  workspaceKey,
} from './helpers';

test('an author edits their own comment', async ({ page }) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Talk about it');
  await page.goto(`/browse/${workspaceKey(page)}-1`);

  await page.getByTestId('add-comment').click();
  await page
    .getByTestId('comment-editor')
    .locator('[contenteditable]')
    .fill('First thoughts');
  await page.getByTestId('post-comment').click();
  const comments = page.getByTestId('activities');
  await expect(comments).toContainText('First thoughts');

  await comments.getByTestId('edit-comment').click();
  const editor = comments.getByTestId('comment-edit-editor');
  await expect(editor).toContainText('First thoughts');
  await editor.locator('[contenteditable]').fill('Second thoughts');
  await comments.getByTestId('comment-edit-save').click();
  await expect(comments.getByTestId('comment-edit')).toBeHidden();
  await expect(comments).toContainText('Second thoughts');
  await expect(comments).not.toContainText('First thoughts');

  // It is stored, not only shown.
  await page.reload();
  await expect(page.getByTestId('activities')).toContainText('Second thoughts');
});
