import { expect, test } from '@playwright/test';
import {
  createIssue,
  createWorkspaceSpaceList,
  login,
  register,
} from './helpers';

test('comments are rich text; links are limited to safe addresses and markup stays text', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Rich comments');
  await page.getByTestId('card-Rich comments').click();
  const issue = page.getByTestId('issue-dialog');
  await issue.getByTestId('add-comment').click();
  const editor = issue.getByTestId('comment-editor');
  const box = editor.getByRole('textbox', { name: 'Comment' });
  await box.click();
  await page.keyboard.press('ControlOrMeta+b');
  await page.keyboard.type('Important');
  await page.keyboard.press('ControlOrMeta+b');
  await page.keyboard.type(' note <img src=x onerror=alert(1)> unsafe');
  // A link with a dangerous address is refused by the editor...
  for (let i = 0; i < 6; i++) await page.keyboard.press('Shift+ArrowLeft');
  page.once('dialog', (d) => d.accept('javascript:alert(1)'));
  await editor.getByTestId('rich-link').click();
  await expect(box).toBeFocused();
  await expect(box.locator('a')).toHaveCount(0);
  // ...a normal one is kept.
  await page.keyboard.press('ArrowRight');
  await page.keyboard.type(' and docs');
  for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowLeft');
  page.once('dialog', (d) => d.accept('https://example.test/docs'));
  await editor.getByTestId('rich-link').click();
  await expect(box.locator('a')).toHaveCount(1);
  const answer = page.waitForResponse(
    (r) => r.url().includes('/activities') && r.request().method() === 'POST',
  );
  await issue.getByTestId('post-comment').click();
  const res = await answer;
  expect(
    res.status(),
    `${res.request().postData()} -> ${await res.text()}`,
  ).toBe(200);

  const posted = issue
    .getByTestId('activities')
    .getByTestId('rich-view')
    .first();
  await expect(posted.locator('strong')).toHaveText('Important');
  await expect(posted).toContainText('<img src=x onerror=alert(1)>'); // text, never markup
  await expect(posted.locator('img')).toHaveCount(0);
  await expect(posted.locator('a')).toHaveCount(1);
  await expect(posted.locator('a')).toHaveAttribute(
    'href',
    'https://example.test/docs',
  );
  await expect(posted.locator('a')).toHaveAttribute('rel', /noopener/);
});

test('two people edit one description at the same time and both see it live', async ({
  page,
  browser,
}) => {
  const user = await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Shared spec');
  await page.getByTestId('card-Shared spec').click();
  await page.getByTestId('issue-key').click(); // open the full page of the issue
  await expect(page).toHaveURL(/\/browse\//);
  const url = page.url();

  const other = await (await browser.newContext()).newPage();
  await login(other, user);
  await other.waitForURL((u) => !u.pathname.startsWith('/login'));
  await other.goto(url);

  const a = page.getByTestId('issue-description');
  const b = other.getByTestId('issue-description');
  await expect(a.getByTestId('collab-status')).toContainText('Live');
  await expect(b.getByTestId('collab-status')).toContainText('Live');

  await a.getByRole('textbox', { name: 'Description' }).click();
  await page.keyboard.type('Hello from A');
  await expect(b.getByRole('textbox', { name: 'Description' })).toContainText(
    'Hello from A',
  );

  // B continues in the middle of the live document; A sees it, and nothing is lost or doubled.
  await b.getByRole('textbox', { name: 'Description' }).click();
  await other.keyboard.press('Control+End');
  await other.keyboard.type(' and B');
  await expect(a.getByRole('textbox', { name: 'Description' })).toHaveText(
    'Hello from A and B',
  );
  await expect(b.getByRole('textbox', { name: 'Description' })).toHaveText(
    'Hello from A and B',
  );

  // Formatting travels too.
  await b.getByTestId('rich-bold').click();
  await other.keyboard.type('X');
  await expect(a.locator('strong')).toHaveText('X');

  // The text is saved into the card: a fresh visit shows it without any live session.
  await expect(a.getByTestId('collab-status')).toContainText('saved', {
    timeout: 15_000,
  });
  const fresh = await (await browser.newContext()).newPage();
  await login(fresh, user);
  await fresh.waitForURL((u) => !u.pathname.startsWith('/login'));
  await fresh.goto(url);
  await expect(
    fresh
      .getByTestId('issue-description')
      .getByRole('textbox', { name: 'Description' }),
  ).toHaveText('Hello from A and BX');
});

test('markdown shortcuts work in the description and in comments', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Markdown');
  await page.getByTestId('card-Markdown').click();
  const issue = page.getByTestId('issue-dialog');

  const description = issue
    .getByTestId('issue-description')
    .getByRole('textbox')
    .first();
  await description.click();
  await page.keyboard.type('# Title ');
  await page.keyboard.type('first line');
  await page.keyboard.press('Enter');
  await page.keyboard.type('## Sub ');
  await page.keyboard.type('second');
  await page.keyboard.press('Enter');
  await page.keyboard.type('- item');
  await page.keyboard.press('Enter');
  await page.keyboard.type('more');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await page.keyboard.type('1. one');
  await page.keyboard.press('Enter');
  await page.keyboard.type('two');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await page.keyboard.type('> quoted');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await page.keyboard.type('**bold** and `code` and ~~gone~~ ');
  await expect(description.locator('h1')).toHaveText('Title first line');
  await expect(description.locator('h2')).toHaveText('Sub second');
  await expect(description.locator('ul li')).toHaveCount(2);
  await expect(description.locator('ol li')).toHaveCount(2);
  await expect(description.locator('blockquote')).toHaveText('quoted');
  await expect(description.locator('strong')).toHaveText('bold');
  await expect(description.locator('code')).toHaveText('code');
  await expect(description.locator('s')).toHaveText('gone');
});

test('pasted Markdown becomes formatted content', async ({ page }) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  await createIssue(page, 'Pasted');
  await page.getByTestId('card-Pasted').click();
  const issue = page.getByTestId('issue-dialog');
  const description = issue
    .getByTestId('issue-description')
    .getByRole('textbox')
    .first();
  await description.click();
  await description.evaluate((el) => {
    const data = new DataTransfer();
    data.setData(
      'text/plain',
      '# Pasted title\n\n- one\n- two\n\nSee [docs](https://example.test) and [bad](javascript:alert(1))',
    );
    el.dispatchEvent(
      new ClipboardEvent('paste', {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect(description.locator('h1')).toHaveText('Pasted title');
  await expect(description.locator('ul li')).toHaveCount(2);
  await expect(description.locator('a')).toHaveCount(1);
  await expect(description.locator('a')).toHaveAttribute(
    'href',
    'https://example.test',
  );
});
