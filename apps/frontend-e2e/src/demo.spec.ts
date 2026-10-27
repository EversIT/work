import { expect, test } from '@playwright/test';

// The demo is the frontend built with NEXT_PUBLIC_DEMO=true: no backend, made-up data, nothing
// stored. Run it against a demo instance (`npx nx run demo:dev`, or the work-demo image):
//   E2E_DEMO_URL=http://localhost:3100 npx playwright test --config=apps/frontend-e2e/playwright.config.ts demo
const DEMO = process.env.E2E_DEMO_URL;
test.skip(!DEMO, 'set E2E_DEMO_URL to a running demo');
test.use({ baseURL: DEMO });

// The demo is served below a path (`/demo`); keep it in front of every address.
const BASE = DEMO ? new URL(DEMO).pathname.replace(/\/$/, '') : '';
const BURNDOWN = `${BASE}/browse/ACME-2`;

test('the demo opens signed in, with made-up data, and talks to no server', async ({
  page,
}) => {
  const api: string[] = [];
  const errors: string[] = [];
  page.on('request', (r) => {
    if (r.url().includes('/api/v1/')) api.push(r.url());
  });
  page.on('pageerror', (e) => errors.push(e.message));

  await page.goto(`${BASE}/`);
  await expect(page.getByTestId('demo-banner')).toBeVisible();
  // Straight in, no sign-in: the projects and their issues are there.
  await page.goto(`${BASE}/projects/ACME/lists/platform/backlog`);
  await expect(
    page.getByText('Burndown chart for the active sprint'),
  ).toBeVisible();
  await page.goto(`${BASE}/projects/ACME/lists/mobile-app`);
  await expect(page.getByText('Offline mode for issue lists')).toBeVisible();
  expect(api, 'API calls are answered in the browser').toEqual([]);
  expect(errors).toEqual([]);
});

test('changes work in the page, and a reload starts over', async ({ page }) => {
  await page.goto(BURNDOWN);
  await expect(
    page.getByText('I would start with the ideal line'),
  ).toBeVisible();

  await page.getByTestId('add-comment').click();
  await page
    .getByTestId('comment-editor')
    .locator('[contenteditable]')
    .fill('Looks good to me');
  await page.getByTestId('post-comment').click();
  await expect(page.getByTestId('activities')).toContainText(
    'Looks good to me',
  );

  await page.reload();
  await expect(
    page.getByText('I would start with the ideal line'),
  ).toBeVisible();
  await expect(page.getByTestId('activities')).not.toContainText(
    'Looks good to me',
  );
});
