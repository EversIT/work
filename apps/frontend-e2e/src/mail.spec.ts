import { expect, test } from '@playwright/test';
import { login, newUser, register } from './helpers';

// These tests read the messages the backend sends through a local Mailpit (the dev stack and CI
// start one). Without it they skip themselves.
const MAILPIT = process.env.E2E_MAILPIT_API ?? 'http://127.0.0.1:58025';

async function mailFor(
  request: import('@playwright/test').APIRequestContext,
  to: string,
) {
  for (let i = 0; i < 60; i++) {
    const found = await request
      .get(`${MAILPIT}/api/v1/search`, { params: { query: `to:${to}` } })
      .then((r) => r.json())
      .catch(() => ({ messages: [] }));
    const first = found.messages?.[0];
    if (first) {
      const full = await (
        await request.get(`${MAILPIT}/api/v1/message/${first.ID}`)
      ).json();
      return {
        subject: full.Subject as string,
        text: full.Text as string,
        html: full.HTML as string,
      };
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  return null;
}

async function mailpitUp(
  request: import('@playwright/test').APIRequestContext,
) {
  return request
    .get(`${MAILPIT}/api/v1/info`)
    .then((r) => r.ok())
    .catch(() => false);
}

test('forgot password: e-mail link, single use, old sessions are signed out', async ({
  page,
  request,
  browser,
}) => {
  test.skip(!(await mailpitUp(request)), 'Mailpit is not running');
  const u = newUser();
  // A second, already signed-in browser: it must lose its session when the password is reset.
  const other = await (await browser.newContext()).newPage();
  await register(other, u);

  await page.goto('/login');
  await page.getByTestId('forgot-link').click();
  await page.waitForURL(/forgot-password/);
  await page.waitForLoadState('networkidle'); // the dev server may reload once after compiling the route
  await page.getByLabel('Email').fill(u.email);
  await page.getByRole('button', { name: 'Send reset link' }).click();
  await expect(page.getByTestId('reset-sent')).toContainText(
    'If an account exists',
  );

  // The same message appears for an address nobody registered, and no mail is sent to it.
  const ghost = `ghost-${Date.now()}@example.test`;
  await page.goto('/forgot-password');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('Email').fill(ghost);
  await page.getByRole('button', { name: 'Send reset link' }).click();
  await expect(page.getByTestId('reset-sent')).toBeVisible();

  const mail = await mailFor(request, u.email);
  expect(mail?.subject).toContain('Reset');
  expect(await mailFor(request, ghost)).toBeNull();
  const link = mail!.text.match(
    /https?:\/\/\S+reset-password\?token=[\w-]+/,
  )![0];
  const token = new URL(link).searchParams.get('token')!;

  const next = `${u.password}-new`;
  await page.goto(`/reset-password?token=${token}`);
  await page.waitForLoadState('networkidle');
  await page.getByLabel('New password').fill(next);
  await page.getByRole('button', { name: 'Set password' }).click();
  await expect(page.getByTestId('reset-done')).toBeVisible();

  // Reusing the link fails.
  await page.goto(`/reset-password?token=${token}`);
  await page.waitForLoadState('networkidle');
  await page.getByLabel('New password').fill(`${next}-again`);
  await page.getByRole('button', { name: 'Set password' }).click();
  await expect(page.getByText('invalid or has expired')).toBeVisible();

  // New password works; the other browser's session is gone.
  await login(page, { email: u.email, password: next });
  await expect(page.getByText('Create your first workspace')).toBeVisible();
  await other.reload();
  await expect(other).toHaveURL(/\/login/);
});

test('invitation by e-mail only works for the invited address', async ({
  browser,
  request,
}) => {
  test.skip(!(await mailpitUp(request)), 'Mailpit is not running');
  const owner = await (await browser.newContext()).newPage();
  await register(owner);
  await owner.getByLabel('Workspace name').fill('Acme');
  await owner.getByRole('button', { name: 'Create workspace' }).click();
  await expect(owner.getByTestId('workspace-title')).toHaveText('Acme');
  await owner.getByTestId('workspace-settings-link').click();
  await owner.getByTestId('tab-people').click();

  const guest = newUser();
  await owner.getByTestId('invite').click();
  const d = owner.getByRole('dialog');
  await d.getByLabel('Or invite by e-mail').fill(guest.email);
  await d.getByRole('button', { name: 'Create invite' }).click();
  await expect(d.getByTestId('mail-invited')).toBeVisible();

  const mail = await mailFor(request, guest.email);
  expect(mail?.subject).toContain('invited you');
  const link = mail!.text.match(/https?:\/\/\S+invite\?token=[\w-]+/)![0];

  // Somebody else cannot use the link.
  const thief = await (await browser.newContext()).newPage();
  await register(thief);
  await thief.goto(new URL(link).pathname + new URL(link).search);
  await thief.getByTestId('accept-invite').click();
  await expect(thief.getByText('invalid, expired or used up')).toBeVisible();

  // The invited address can.
  const g = await (await browser.newContext()).newPage();
  await register(g, guest);
  await g.goto(new URL(link).pathname + new URL(link).search);
  await g.getByTestId('accept-invite').click();
  await expect(g.getByTestId('workspace-title')).toHaveText('Acme');
});
