import { expect, test, type Locator } from '@playwright/test';
import { createWorkspaceSpaceList, register } from './helpers';

async function html5DragAndDrop(source: Locator, target: Locator) {
  await source.scrollIntoViewIfNeeded();
  await target.scrollIntoViewIfNeeded();

  const sourceHandle = await source.elementHandle();
  const targetHandle = await target.elementHandle();
  if (!sourceHandle || !targetHandle) {
    throw new Error(
      'Could not resolve element handles for HTML5 drag-and-drop',
    );
  }

  await sourceHandle.evaluate((src, dst) => {
    const dataTransfer = new DataTransfer();

    src.dispatchEvent(
      new DragEvent('dragstart', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );

    dst.dispatchEvent(
      new DragEvent('dragenter', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );

    dst.dispatchEvent(
      new DragEvent('dragover', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );

    dst.dispatchEvent(
      new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );

    src.dispatchEvent(
      new DragEvent('dragend', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );
  }, targetHandle);
}

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
  await html5DragAndDrop(
    page.getByTestId('node-step-0-grip'),
    page.getByTestId('connector-2'),
  );
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
