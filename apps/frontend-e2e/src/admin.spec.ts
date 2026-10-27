import { expect, test } from '@playwright/test';
import { register } from './helpers';

// The first account of an instance administers it, and this file runs first and alone (the
// `admin` project in playwright.config.ts), so its user is that administrator on a fresh stack.
const PNG_1X1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

test('the administrator sees the instance and changes its name, colour and favicon', async ({
  page,
}) => {
  const u = await register(page);
  await page.getByTestId('user-menu').click();
  const admin = page.getByRole('menuitem', { name: 'Administration' });
  test.skip(
    !(await admin.isVisible()),
    'needs a fresh instance: only its first account is an administrator',
  );
  await admin.click();
  await expect(page.getByTestId('admin-page')).toBeVisible();

  // Overview: counts and configuration, no secrets.
  await expect(page.getByTestId('stat-users')).not.toHaveText('0');
  await expect(page.getByTestId('admin-page')).not.toContainText(
    /jwt|password_hash|token_hash/i,
  );

  // Users: the account is listed with its sign-in methods and cannot be turned off by itself.
  await page.getByTestId('admin-tab-users').click();
  await expect(page.getByTestId('admin-users')).toContainText(u.email);
  await page.getByTestId(`user-actions-${u.email}`).click();
  await expect(
    page.getByRole('menuitem', { name: 'Disable account' }),
  ).toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Escape');
  await page.getByTestId('admin-user-search').fill('nobody-with-this-name');
  await expect(page.getByTestId('admin-users')).toContainText('No users found');

  // Appearance: name, description and accent colour; the favicon.
  await page.getByTestId('admin-tab-appearance').click();
  const form = page.getByTestId('admin-appearance');
  await form.getByLabel('Name of this instance').fill('Acme Tracker');
  await form.getByLabel('Description').fill('Where Acme plans its work');
  await form.getByTestId('accent-input').fill('#1f845a');
  await expect(page.getByTestId('theme-preview')).toHaveCSS(
    '--accent',
    '#1f845a',
  );
  await form.getByTestId('save-branding').click();
  await page.getByTestId('favicon-input').setInputFiles({
    name: 'icon.png',
    mimeType: 'image/png',
    buffer: PNG_1X1,
  });
  await expect(page.getByTestId('favicon-preview')).toHaveAttribute(
    'src',
    /\/api\/v1\/branding\/favicon\?v=/,
  );
  // The navigation bar follows at once: the new name and the uploaded logo.
  await expect(
    page.getByTestId('top-nav').getByTestId('brand-name'),
  ).toHaveText('Acme Tracker');
  await expect(
    page.getByTestId('top-nav').getByTestId('brand-logo').locator('img'),
  ).toHaveAttribute('src', /\/api\/v1\/branding\/favicon\?v=/);

  await page.reload();
  await expect(page).toHaveTitle('Acme Tracker');
  await expect(page.locator('meta[name="description"]')).toHaveAttribute(
    'content',
    'Where Acme plans its work',
  );
  await expect(page.locator('link[rel="icon"]').first()).toHaveAttribute(
    'href',
    /\/api\/v1\/branding\/favicon\?v=/,
  );
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement)
        .getPropertyValue('--accent')
        .trim(),
    ),
  ).toBe('#1f845a');
  // The favicon is a real image served with a locked-down policy.
  const icon = await page.request.get('/api/v1/branding/favicon');
  expect(icon.headers()['content-type']).toBe('image/png');
  expect(icon.headers()['content-security-policy']).toContain('sandbox');

  // The audit log recorded it.
  await page.getByTestId('admin-tab-audit').click();
  await expect(page.getByTestId('admin-audit')).toContainText(
    'branding.update',
  );
  await expect(page.getByTestId('admin-audit')).toContainText(u.email);

  // Put the defaults back for the tests that follow.
  await page.getByTestId('admin-tab-appearance').click();
  await form.getByLabel('Name of this instance').fill('work');
  await form.getByLabel('Description').fill('Open-source work management');
  await form.getByTestId('accent-input').fill('#0c66e4');
  await form.getByTestId('save-branding').click();
  await page.getByRole('button', { name: 'Use the default' }).click();
  await page.reload();
  await expect(page).toHaveTitle('work');
});

test('everybody else gets no admin area', async ({ page }) => {
  await register(page);
  await page.getByTestId('user-menu').click();
  const admin = page.getByRole('menuitem', { name: 'Administration' });
  test.skip(
    await admin.isVisible(),
    'this account is the administrator of a fresh instance',
  );
  await page.keyboard.press('Escape');
  await page.goto('/admin');
  await expect(page.getByTestId('admin-denied')).toBeVisible();
  expect((await page.request.get('/api/v1/admin/users')).status()).toBe(401); // no token in this request at all
});
