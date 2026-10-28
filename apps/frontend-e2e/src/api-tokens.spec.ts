import { expect, test } from '@playwright/test';
import { register } from './helpers';

test('API tokens: created once, work with Bearer and Jira-style Basic auth, read-only and revocable', async ({
  page,
  request,
}) => {
  const u = await register(page);
  await page.getByLabel('Workspace name').fill('Home');
  await page.getByRole('button', { name: 'Create workspace' }).click();
  await expect(page.getByTestId('workspace-title')).toHaveText('Home');
  await page.goto('/settings/api-tokens');

  // The MCP server is explained right below the tokens: address, how to connect, what it can do.
  const mcp = page.getByTestId('mcp-settings');
  await expect(mcp.getByTestId('mcp-url')).toHaveValue(/\/api\/v1\/mcp$/);
  await expect(mcp.getByTestId('mcp-command')).toContainText('claude mcp add');
  await expect(mcp.getByTestId('mcp-tools')).toContainText('create_issue');

  const create = async (
    name: string,
    scope: 'Read only' | 'Read and write',
  ) => {
    await page.getByTestId('create-token').click();
    const d = page.getByRole('dialog');
    await d.getByLabel('Name').fill(name);
    await d.getByTestId('token-scope').click();
    await page.getByRole('option', { name: scope }).click();
    await d.getByRole('button', { name: 'Create', exact: true }).click();
    const token = await d.getByTestId('new-token').inputValue();
    expect(token).toMatch(/^wrk_/);
    await d.getByRole('button', { name: 'Done' }).click();
    return token;
  };
  const writeToken = await create('CI pipeline', 'Read and write');
  const readToken = await create('Dashboard', 'Read only');

  // After the dialog is closed the secret is gone from the page; only a prefix is listed.
  const list = page.getByTestId('token-list');
  await expect(list).toContainText('CI pipeline');
  await expect(list).toContainText(writeToken.slice(0, 8));
  expect(await page.content()).not.toContain(writeToken);
  await page.reload();
  expect(await page.content()).not.toContain(writeToken);

  // The token authenticates against the same origin the browser uses.
  const me = await request.get('/api/v1/me', {
    headers: { Authorization: `Bearer ${writeToken}` },
  });
  expect(me.status()).toBe(200);
  expect((await me.json()).auth_method).toBe('api_token');
  const basic = Buffer.from(`${u.email}:${writeToken}`).toString('base64');
  expect(
    (
      await request.get('/api/v1/me', {
        headers: { Authorization: `Basic ${basic}` },
      })
    ).status(),
  ).toBe(200);
  const wrongUser = Buffer.from(
    `someone-else@example.test:${writeToken}`,
  ).toString('base64');
  expect(
    (
      await request.get('/api/v1/me', {
        headers: { Authorization: `Basic ${wrongUser}` },
      })
    ).status(),
  ).toBe(401);

  // Read-only tokens cannot write; no token can manage tokens or credentials.
  const post = (token: string, url: string, data: unknown) =>
    request.post(url, { headers: { Authorization: `Bearer ${token}` }, data });
  expect(
    (await post(readToken, '/api/v1/projects', { name: 'x' })).status(),
  ).toBe(403);
  expect(
    (
      await post(writeToken, '/api/v1/projects', {
        name: 'Created through the API',
      })
    ).status(),
  ).toBe(200);
  expect(
    (
      await post(writeToken, '/api/v1/me/api-tokens', {
        name: 'x',
        scope: 'write',
      })
    ).status(),
  ).toBe(403);
  expect(
    (
      await request.get('/api/v1/me/api-tokens', {
        headers: { Authorization: `Bearer ${writeToken}` },
      })
    ).status(),
  ).toBe(403);

  // Revoking is immediate.
  page.once('dialog', (d) => d.accept());
  await list
    .locator('li', { hasText: 'CI pipeline' })
    .getByRole('button', { name: 'Revoke' })
    .click();
  await expect(list).not.toContainText('CI pipeline');
  expect(
    (
      await request.get('/api/v1/me', {
        headers: { Authorization: `Bearer ${writeToken}` },
      })
    ).status(),
  ).toBe(401);
  expect(
    (
      await request.get('/api/v1/me', {
        headers: { Authorization: `Bearer ${readToken}` },
      })
    ).status(),
  ).toBe(200);
});
