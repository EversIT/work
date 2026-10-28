import { expect, test } from '@playwright/test';

// The demo is the frontend started with DEMO_MODE=true: no backend, made-up data, nothing
// stored. Run it against a demo instance (`DEMO_MODE=true npx nx run frontend:dev`, or the frontend image):
//   E2E_DEMO_URL=http://localhost:3000 npx playwright test --config=apps/frontend-e2e/playwright.config.ts demo
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

test('linked work items: seeded links show, a new one can be added and removed', async ({
  page,
}) => {
  await page.goto(BURNDOWN);
  const links = page.getByTestId('linked-issues');
  // The demo ships "ACME-2 blocks ACME-3".
  await expect(links.getByTestId('links-blocks')).toContainText('ACME-3');
  await expect(links.getByTestId('link-row')).toHaveCount(1);

  await links.getByTestId('add-link').click();
  await links.getByTestId('link-relation').click();
  await page.getByRole('option', { name: 'relates to', exact: true }).click();
  await links.getByTestId('link-search').fill('ACME-5');
  await links
    .getByTestId('link-results')
    .getByRole('button', { name: /ACME-5/ })
    .click();
  await links.getByTestId('link-submit').click();
  await expect(links.getByTestId('links-relates')).toContainText('ACME-5');
  await expect(links.getByTestId('link-row')).toHaveCount(2);

  // The same relationship again is refused; the form stays open.
  await links.getByTestId('add-link').click();
  await links.getByTestId('link-relation').click();
  await page.getByRole('option', { name: 'relates to', exact: true }).click();
  await links.getByTestId('link-search').fill('ACME-5');
  await links
    .getByTestId('link-results')
    .getByRole('button', { name: /ACME-5/ })
    .click();
  await links.getByTestId('link-submit').click();
  await expect(links.getByTestId('add-link-form')).toBeVisible();
  await expect(links.getByTestId('link-row')).toHaveCount(2);
  await links.getByRole('button', { name: 'Cancel' }).click();

  await links.getByRole('button', { name: 'Remove link to ACME-3' }).click();
  await expect(links.getByTestId('links-blocks')).toHaveCount(0);
  await expect(links.getByTestId('link-row')).toHaveCount(1);
});

test('linked work items: every relationship is accepted', async ({ page }) => {
  await page.goto(BURNDOWN);
  const links = page.getByTestId('linked-issues');
  const all: [label: string, section: string, target: string][] = [
    ['blocks', 'blocks', 'ACME-4'],
    ['is blocked by', 'is_blocked_by', 'ACME-5'],
    ['duplicates', 'duplicates', 'ACME-6'],
    ['is duplicated by', 'is_duplicated_by', 'ACME-7'],
    ['relates to', 'relates', 'ACME-8'],
    ['clones', 'clones', 'ACME-1'],
  ];
  for (const [label, section, target] of all) {
    await links.getByTestId('add-link').click();
    await links.getByTestId('link-relation').click();
    await page.getByRole('option', { name: label, exact: true }).click();
    await links.getByTestId('link-search').fill(target);
    await links
      .getByTestId('link-results')
      .getByRole('button', { name: new RegExp(`${target}(?!\\d)`) })
      .click();
    await links.getByTestId('link-submit').click();
    // No "unknown link type": the form closes and the issue shows up under its own wording.
    await expect(links.getByTestId('add-link-form')).toBeHidden();
    await expect(links.getByTestId(`links-${section}`)).toContainText(target);
  }
});
