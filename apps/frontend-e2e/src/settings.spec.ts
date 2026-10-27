import { expect, test } from '@playwright/test';
import { randomBytes } from 'node:crypto';
import { register } from './helpers';

test('sign-in pages follow the browser language and have no language switch', async ({
  browser,
}) => {
  const german = await browser.newContext({ locale: 'de-DE' });
  const page = await german.newPage();
  await page.goto('/login');
  await expect(
    page.getByRole('button', { name: 'Anmelden', exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId('language-select')).toHaveCount(0);
  await page.goto('/register');
  await expect(
    page.getByRole('button', { name: 'Konto erstellen' }),
  ).toBeVisible();
  await expect(page.getByTestId('language-select')).toHaveCount(0);
  await german.close();
});

test('language and theme are chosen in the settings; the language lives in a cookie, never in the URL', async ({
  page,
  context,
}) => {
  await register(page);
  await page.goto('/settings/appearance');
  await expect(page.getByTestId('theme-select')).toBeVisible();
  await page.getByTestId('language-select').click();
  await page.getByRole('option', { name: 'Deutsch' }).click();
  await expect(
    page.getByRole('heading', { name: 'Erscheinungsbild' }),
  ).toBeVisible();
  expect(new URL(page.url()).pathname).toBe('/settings/appearance'); // no /de, no ?lang=
  const cookies = await context.cookies();
  expect(cookies.find((c) => c.name === 'NEXT_LOCALE')?.value).toBe('de');
  await page.reload();
  await expect(
    page.getByRole('heading', { name: 'Erscheinungsbild' }),
  ).toBeVisible();

  // Dark mode: switch it, see the page change, keep it across a reload.
  const html = page.locator('html');
  const background = () =>
    page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const light = await background();
  await page.getByTestId('theme-select').click();
  await page.getByRole('option', { name: 'Dunkel' }).click();
  await expect(html).toHaveClass(/dark/);
  expect(await background()).not.toBe(light);
  await page.reload();
  await expect(html).toHaveClass(/dark/);
  await page.getByTestId('theme-select').click();
  await page.getByRole('option', { name: 'Hell' }).click();
  await expect(html).not.toHaveClass(/dark/);
  expect(await background()).toBe(light);
});

test('profile, password change and account deletion', async ({ page }) => {
  const u = await register(page);
  await page.getByLabel('Workspace name').fill('Home');
  await page.getByRole('button', { name: 'Create workspace' }).click();
  await expect(page.getByTestId('workspace-title')).toHaveText('Home');
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Profile' })).toBeVisible();

  await page.getByLabel('First name').fill('Renamed');
  await page.getByRole('button', { name: 'Save' }).first().click();
  await expect(page.getByText('Saved')).toBeVisible();

  // The password is under "Sign-in and security".
  await page.getByTestId('atab-security').click();
  const next = randomBytes(18).toString('base64url');
  await page
    .getByLabel('Current password')
    .first()
    .fill('definitely-wrong-password');
  await page.getByLabel('New password').fill(next);
  await page.getByTestId('change-password').click();
  await expect(page.getByText('The password is incorrect.')).toBeVisible();
  await page.getByLabel('Current password').first().fill(u.password);
  await page.getByTestId('change-password').click();
  await expect(page.getByText(/Password changed/)).toBeVisible();

  // Deleting the account is on the profile page.
  await page.getByTestId('atab-profile').click();
  await page.getByTestId('delete-account').click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Current password').fill(next);
  await dialog.getByRole('button', { name: 'Delete account' }).click();
  await expect(page).toHaveURL(/\/login/);
  // The account is gone: the old credentials no longer work.
  await page.getByLabel('Email').fill(u.email);
  await page.getByLabel('Password').fill(next);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByText('Email or password is incorrect.')).toBeVisible();
});

test('security headers and no secrets in the browser storage', async ({
  page,
}) => {
  const resp = await page.goto('/login');
  const h = resp?.headers() ?? {};
  expect(h['x-frame-options']).toBe('DENY');
  expect(h['x-content-type-options']).toBe('nosniff');
  expect(h['content-security-policy']).toContain("frame-ancestors 'none'");
  await register(page);
  const storage = await page.evaluate(() =>
    JSON.stringify({ l: { ...localStorage }, s: { ...sessionStorage } }),
  );
  expect(storage).not.toMatch(/eyJ[A-Za-z0-9_-]{10,}\./); // no JWT in web storage
  expect(await page.evaluate(() => document.cookie)).not.toContain(
    'work_refresh',
  ); // HttpOnly: invisible to scripts
});
