import { expect, test } from '@playwright/test';
import { login, mailLink, newUser, register, totp } from './helpers';

const MAILPIT = process.env.E2E_MAILPIT_API ?? 'http://127.0.0.1:58025';

async function mailpitUp(
  request: import('@playwright/test').APIRequestContext,
) {
  return request
    .get(`${MAILPIT}/api/v1/info`)
    .then((r) => r.ok())
    .catch(() => false);
}

test('pages carry a per-request CSP nonce and no inline-script allowance', async ({
  request,
}) => {
  const nonces = new Set<string>();
  for (let i = 0; i < 2; i++) {
    const res = await request.get('/login');
    const csp = res.headers()['content-security-policy'];
    const script = csp
      .split(';')
      .find((d) => d.trim().startsWith('script-src'))!;
    expect(script).not.toContain("'unsafe-inline'");
    const nonce = script.match(/'nonce-([^']+)'/)![1];
    nonces.add(nonce);
    // Next puts the same nonce on its own inline/bootstrap scripts.
    expect(await res.text()).toContain(`nonce="${nonce}"`);
  }
  expect(nonces.size).toBe(2);
});

test('the app still hydrates under the strict CSP', async ({ page }) => {
  const violations: string[] = [];
  page.on('console', (m) => {
    if (/Content Security Policy|Refused to/.test(m.text()))
      violations.push(m.text());
  });
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  // A client-side-only interaction proves the scripts ran.
  await page
    .getByTestId('forgot-link')
    .or(page.getByRole('link', { name: /Register/ }))
    .first()
    .click();
  await expect(page).toHaveURL(/forgot-password|register/);
  expect(violations).toEqual([]);
});

test('sign-up needs the confirmation link when e-mail verification is on', async ({
  page,
  request,
}) => {
  test.skip(!(await mailpitUp(request)), 'Mailpit is not running');
  const info = await (await request.get('/api/v1/system/info')).json();
  test.skip(
    !info.email_verification_required,
    'e-mail verification is off on this stack',
  );
  const u = newUser();
  await page.goto('/register');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('First name').fill(u.first);
  await page.getByLabel('Last name').fill(u.last);
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(u.password);
  await page.getByRole('button', { name: 'Create account' }).click();
  await expect(page.getByTestId('confirm-sent')).toBeVisible();

  // No account yet: signing in fails.
  await login(page, u);
  await expect(page.getByText('incorrect')).toBeVisible();

  const link = await mailLink(request, u.email, 'verify-email');
  await page.goto(link);
  await expect(page.getByText('Create your first workspace')).toBeVisible();

  // The link works once.
  const again = await page.context().newPage();
  await again.goto(link);
  await expect(again.getByTestId('verify-error')).toBeVisible();
});

test('weak passwords are refused with a reason', async ({ page }) => {
  const u = newUser();
  await page.goto('/register');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('First name').fill(u.first);
  await page.getByLabel('Last name').fill(u.last);
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill('password1234');
  await page.getByRole('button', { name: 'Create account' }).click();
  await expect(page.getByText('too common')).toBeVisible();
});

test('repeated wrong passwords lock the account, even for the right password', async ({
  page,
  browser,
}) => {
  const u = newUser();
  const other = await (await browser.newContext()).newPage();
  await register(other, u);
  for (let i = 0; i < 5; i++) {
    await login(page, { email: u.email, password: `${u.password}-wrong` });
    await expect(page.getByText('incorrect')).toBeVisible();
  }
  await login(page, u);
  await expect(page.getByText('incorrect')).toBeVisible();
});

test('two-factor authentication: set up, sign in with a code, recovery code', async ({
  page,
  browser,
}) => {
  const info = await (await page.request.get('/api/v1/system/info')).json();
  test.skip(!info.mfa_available, 'the stack has no WORK_ENCRYPTION_KEY');
  const u = newUser();
  await register(page, u);
  await page.goto('/settings/security'); // the realtime stream keeps the network busy, so wait for the card instead
  await page.getByTestId('mfa-enable').click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Password').fill(u.password);
  await dialog.getByRole('button', { name: 'Continue' }).click();
  const secret = (await dialog.getByTestId('mfa-secret').innerText()).trim();
  await expect(dialog.getByRole('img', { name: /QR code/ })).toBeVisible();
  await dialog.getByLabel('6-digit code').fill(totp(secret));
  await dialog.getByRole('button', { name: 'Turn on' }).click();
  const codes = (await dialog.getByTestId('recovery-codes').innerText())
    .split('\n')
    .map((c) => c.trim())
    .filter(Boolean);
  expect(codes).toHaveLength(10);
  await dialog.getByRole('button', { name: 'Done' }).click();
  await expect(
    page.getByTestId('mfa-settings').getByText('On', { exact: true }),
  ).toBeVisible();

  // A fresh browser: the password alone is not enough; a wrong code is refused.
  const fresh = await (await browser.newContext()).newPage();
  await login(fresh, u);
  await expect(fresh.getByTestId('mfa-code')).toBeVisible();
  await fresh.getByTestId('mfa-code').fill('000000');
  await fresh.getByTestId('mfa-submit').click();
  await expect(fresh.getByText('not correct')).toBeVisible();

  // The code that enabled MFA is spent, the next time step is accepted.
  await fresh.getByTestId('mfa-code').fill(totp(secret, Date.now() + 30_000));
  await fresh.getByTestId('mfa-submit').click();
  await expect(fresh.getByText('Create your first workspace')).toBeVisible();

  // A recovery code works once.
  const viaRecovery = await (await browser.newContext()).newPage();
  await login(viaRecovery, u);
  await viaRecovery.getByTestId('mfa-code').fill(codes[0]);
  await viaRecovery.getByTestId('mfa-submit').click();
  await expect(
    viaRecovery.getByText('Create your first workspace'),
  ).toBeVisible();
  const reuse = await (await browser.newContext()).newPage();
  await login(reuse, u);
  await reuse.getByTestId('mfa-code').fill(codes[0]);
  await reuse.getByTestId('mfa-submit').click();
  await expect(reuse.getByText('not correct')).toBeVisible();
});

test('passkeys: add one in the settings and sign in with it, no password typed', async ({
  page,
  context,
}) => {
  // Chrome's virtual authenticator stands in for a phone or security key.
  const cdp = await context.newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  const u = await register(page);
  await page.goto('/settings/security');
  await page.getByTestId('add-passkey').click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name').fill('Test key');
  await dialog.getByLabel('Your password').fill(u.password);
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await expect(page.getByTestId('passkey-list')).toContainText('Test key');

  // A fresh browser state: the passkey alone signs in.
  await context.clearCookies();
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByTestId('passkey-login').click();
  await expect(page.getByText('Create your first workspace')).toBeVisible();
  await page.goto('/settings/security');
  await expect(page.getByTestId('passkey-list')).toContainText('used');
});
