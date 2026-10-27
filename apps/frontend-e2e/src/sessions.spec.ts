import { expect, test } from '@playwright/test';
import { login, register } from './helpers';

const FIREFOX_ON_WINDOWS =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0';

test('sessions: listed with device, ended one by one or all at once, and dead right away', async ({
  page,
  browser,
  request,
}) => {
  const u = await register(page);
  await page.goto('/settings/sessions');
  const list = page.getByTestId('session-list');
  await expect(list.locator('li')).toHaveCount(1);
  await expect(list).toContainText('this device');
  // The current session cannot be ended from its own list (that is what "Sign out" is for).
  await expect(list.getByRole('button')).toHaveCount(0);

  // A second sign-in from another device shows up with a readable device name.
  const other = await browser.newContext({ userAgent: FIREFOX_ON_WINDOWS });
  const otherPage = await other.newPage();
  await login(otherPage, u);
  await expect(otherPage).not.toHaveURL(/\/login/);
  await page.reload();
  await expect(list.locator('li')).toHaveCount(2);
  await expect(list).toContainText('Firefox · Windows');
  await expect(list.locator('li', { hasText: 'this device' })).toHaveCount(1);

  // Ending it signs that device out: its refresh cookie no longer works.
  await list
    .locator('li', { hasText: 'Firefox' })
    .getByRole('button', { name: 'Sign out this session' })
    .click();
  await expect(list.locator('li')).toHaveCount(1);
  await otherPage.goto('/settings/sessions');
  await expect(otherPage).toHaveURL(/\/login/);

  // "Sign out everywhere else" ends every other sign-in but keeps this one.
  const revokeOthers = page.getByTestId('revoke-other-sessions');
  await expect(revokeOthers).toBeDisabled();
  await login(otherPage, u);
  await expect(otherPage).not.toHaveURL(/\/login/);
  await page.reload();
  await expect(list.locator('li')).toHaveCount(2);
  page.once('dialog', (d) => d.accept());
  await revokeOthers.click();
  await expect(list.locator('li')).toHaveCount(1);
  await expect(list).toContainText('this device');
  await otherPage.goto('/settings/sessions');
  await expect(otherPage).toHaveURL(/\/login/);
  await other.close();

  // This session still works.
  await page.reload();
  await expect(page.getByTestId('session-settings')).toBeVisible();

  // The endpoints are for signed-in browsers only.
  expect((await request.get('/api/v1/me/sessions')).status()).toBe(401);
});
