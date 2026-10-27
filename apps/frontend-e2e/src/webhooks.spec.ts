import { expect, test, type Page } from '@playwright/test';
import { createHmac } from 'node:crypto';
import { createServer, type IncomingHttpHeaders } from 'node:http';
import {
  createIssue,
  createWorkspaceSpaceList,
  register,
  workspaceKey,
} from './helpers';

/** Opens the rule builder for a new rule (workspace settings, Automations, New automation). */
async function openBuilder(page: Page) {
  await page.getByTestId('workspace-settings-link').click();
  await page.getByTestId('tab-automations').click();
  await page.getByTestId('new-automation').first().click();
  await expect(page.getByTestId('automation-builder')).toBeVisible();
}

/** Saves the rule and goes back to the list of rules. */
async function saveAndLeave(page: Page) {
  await page.getByTestId('builder-save').click();
  await expect(page.getByTestId('builder-dirty')).toHaveCount(0);
  await page.getByTestId('builder-back').click();
}

test('incoming webhook creates an issue; rotating the URL invalidates the old one', async ({
  page,
  request,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);

  await openBuilder(page);
  await page.getByLabel('Rule name').fill('From monitoring');
  await page.getByTestId('node-trigger-select').click();
  await page.getByTestId('trigger-type').click();
  await page.getByRole('option', { name: 'Incoming webhook' }).click();
  await page.getByTestId('node-step-0-select').click();
  await page.getByTestId('action-type-0').click();
  await page.getByRole('option', { name: 'Create an issue' }).click();
  await page.getByLabel('Issue summary').fill('Alert: {{trigger.body.title}}');
  await saveAndLeave(page);
  await expect(page.getByTestId('automation-From monitoring')).toBeVisible();

  // The URL is shown exactly once.
  await page.getByTestId('automation-menu-From monitoring').click();
  await page.getByTestId('webhook-url-From monitoring').click();
  const url = await page.getByTestId('secret-value').inputValue();
  expect(url).toMatch(/\/api\/v1\/hooks\/[0-9a-f-]{36}\/[\w-]{20,}$/);
  await page.getByRole('dialog').getByRole('button', { name: 'Done' }).click();
  expect(await page.content()).not.toContain(url.split('/').pop()!);

  const hook = new URL(url);
  const call = (path: string, data: unknown) => request.post(path, { data });
  expect((await call(hook.pathname, { title: 'Deploy failed' })).status()).toBe(
    202,
  );
  expect(
    (
      await call(hook.pathname.replace(/[\w-]+$/, 'wrong-secret'), {
        title: 'x',
      })
    ).status(),
  ).toBe(404);

  await page
    .getByTestId('side-nav')
    .getByRole('link', { name: 'Back' })
    .or(page.getByTestId('projects-menu'))
    .first()
    .click();
  await page.getByRole('menuitem', { name: /Backlog/ }).click();
  await expect(page.getByTestId('card-Alert: Deploy failed')).toBeVisible({
    timeout: 15_000,
  });

  // Rotation.
  await page.getByTestId('workspace-settings-link').click();
  await page.getByTestId('tab-automations').click();
  page.once('dialog', (x) => x.accept());
  await page.getByTestId('automation-menu-From monitoring').click();
  await page.getByTestId('webhook-url-From monitoring').click();
  const rotated = new URL(await page.getByTestId('secret-value').inputValue());
  expect(rotated.pathname).not.toBe(hook.pathname);
  expect((await call(hook.pathname, { title: 'old' })).status()).toBe(404);
  expect((await call(rotated.pathname, { title: 'new' })).status()).toBe(202);
});

// Needs a backend that may call back into the test host (WORK_WEBHOOK_ALLOW_PRIVATE/HTTP=true, same network).
// Against the hardened Docker stack the SSRF guard rightly refuses the target, so skip it there.
test('outgoing webhook is signed with a key the admin can read', async ({
  page,
}) => {
  test.skip(
    !!process.env.E2E_SKIP_OUTGOING,
    'backend cannot reach the test host',
  );
  type Hit = { headers: IncomingHttpHeaders; body: string };
  const hits: Hit[] = [];
  const server = createServer((req, res) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      hits.push({ headers: req.headers, body });
      res.writeHead(200).end('ok');
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const port = (server.address() as { port: number }).port;

  try {
    await register(page);
    await createWorkspaceSpaceList(page);
    await openBuilder(page);
    await page.getByLabel('Rule name').fill('Notify chat');
    await page.getByTestId('node-step-0-select').click();
    await page.getByTestId('action-type-0').click();
    await page.getByRole('option', { name: 'Send a webhook' }).click();
    await page.getByLabel('URL').fill(`http://127.0.0.1:${port}/hook`);
    await page
      .getByLabel('Payload (JSON, optional)')
      .fill(
        '{"issue": "{{trigger.card.key}}", "title": "{{trigger.card.data.title}}"}',
      );
    await saveAndLeave(page);
    await expect(page.getByTestId('automation-Notify chat')).toBeVisible();

    await page.getByTestId('projects-menu').click();
    await page.getByRole('menuitem', { name: /Backlog/ }).click();
    await createIssue(page, 'Ship it');
    await expect
      .poll(() => hits.length, { timeout: 15_000 })
      .toBeGreaterThan(0);

    const hit = hits[0];
    const payload = JSON.parse(hit.body);
    expect(payload.data).toEqual({
      issue: `${workspaceKey(page)}-1`,
      title: 'Ship it',
    });
    expect(hit.headers['x-work-event']).toBe('automation.run');

    // Read the signing secret in the UI and verify the signature like a receiver would.
    await page.getByTestId('workspace-settings-link').click();
    await page.getByTestId('tab-automations').click();
    await page.getByTestId('automation-menu-Notify chat').click();
    await page.getByTestId('signing-secret-Notify chat').click();
    const secret = await page.getByTestId('secret-value').inputValue();
    const expected = `sha256=${createHmac('sha256', Buffer.from(secret, 'hex')).update(hit.body).digest('hex')}`;
    expect(hit.headers['x-work-signature']).toBe(expected);
  } finally {
    server.close();
  }
});
