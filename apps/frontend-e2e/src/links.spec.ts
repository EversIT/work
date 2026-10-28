import { expect, test, type Page } from '@playwright/test';
import {
  createIssue,
  createWorkspaceSpaceList,
  finishSignUp,
  newUser,
  register,
  workspaceKey,
} from './helpers';

/** Adds a link from the open issue page: relationship, then the issue found by `search`. */
async function addLink(
  page: Page,
  relation: string,
  search: string,
  pick: string,
) {
  const links = page.getByTestId('linked-issues');
  await links.getByTestId('add-link').click();
  await links.getByTestId('link-relation').click();
  await page.getByRole('option', { name: relation, exact: true }).click();
  await links.getByTestId('link-search').fill(search);
  await links
    .getByTestId('link-results')
    .getByRole('button', { name: new RegExp(pick) })
    .click();
  await links.getByTestId('link-submit').click();
}

test('linked work items: link, read from both ends, refuse duplicates, remove', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  const key = workspaceKey(page);
  await createIssue(page, 'Blocker');
  await createIssue(page, 'Target');
  await createIssue(page, 'Sibling');
  const [blocker, target, sibling] = [`${key}-1`, `${key}-2`, `${key}-3`];

  // "Blocker blocks Target", found by key.
  await page.goto(`/browse/${blocker}`);
  const links = page.getByTestId('linked-issues');
  await expect(links.getByTestId('link-row')).toHaveCount(0);
  await addLink(page, 'blocks', target, target);
  const blocks = links.getByTestId('links-blocks');
  await expect(blocks).toContainText('Target');
  await expect(blocks).toContainText(target);
  await expect(links.getByTestId('add-link-form')).toBeHidden();

  // The other end reads the same link the other way round, and it survives a reload.
  await page.goto(`/browse/${target}`);
  await expect(page.getByTestId('links-is_blocked_by')).toContainText(
    'Blocker',
  );
  await page.reload();
  await expect(page.getByTestId('links-is_blocked_by')).toContainText(blocker);

  // Both issues got a history entry.
  await page.getByRole('tab', { name: 'History' }).click();
  await expect(page.getByTestId('activities')).toContainText(
    `linked: is blocked by ${blocker}`,
  );

  // The reverse ("Target blocks Blocker") would contradict it: refused, the form stays open.
  await addLink(page, 'blocks', blocker, blocker);
  await expect(links.getByTestId('add-link-form')).toBeVisible();
  await expect(links.getByTestId('link-row')).toHaveCount(1);
  await links.getByRole('button', { name: 'Cancel' }).click();

  // Another relationship to the same issue is fine, and the search also finds by summary.
  await addLink(page, 'relates to', 'Sibling', sibling);
  await expect(page.getByTestId('links-relates')).toContainText('Sibling');
  await expect(links.getByTestId('link-row')).toHaveCount(2);

  // An issue is not offered as a link to itself.
  await links.getByTestId('add-link').click();
  await links.getByTestId('link-search').fill(target);
  await expect(links.getByTestId('link-results')).toBeHidden();
  await links.getByRole('button', { name: 'Cancel' }).click();

  // Removing from one end removes it for both.
  await links
    .getByRole('button', { name: `Remove link to ${blocker}` })
    .click();
  await expect(links.getByTestId('link-row')).toHaveCount(1);
  await page.goto(`/browse/${blocker}`);
  await expect(links.getByTestId('links-blocks')).toHaveCount(0);
  await expect(links.getByTestId('links-relates')).toHaveCount(0);
});

test('linked work items: a viewer sees the links but cannot change them', async ({
  page,
  browser,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  const key = workspaceKey(page);
  await createIssue(page, 'Left');
  await createIssue(page, 'Right');
  await page.goto(`/browse/${key}-1`);
  await addLink(page, 'duplicates', `${key}-2`, `${key}-2`);
  await expect(page.getByTestId('links-duplicates')).toContainText('Right');

  // Invite a viewer to the project (as in collab.spec.ts) and look at the same issue as them.
  await page.getByTestId('workspace-settings-link').click();
  await page.getByTestId('tab-people').click();
  await page.getByTestId('invite').click();
  const dialog = page.getByRole('dialog');
  await dialog.getByTestId('invite-role').click();
  await page.getByRole('option', { name: 'Viewer', exact: true }).click();
  await dialog.getByRole('button', { name: 'Create invite' }).click();
  const invite = await page.getByTestId('invite-link').inputValue();

  const viewer = await (await browser.newContext()).newPage();
  const vu = newUser();
  await viewer.goto(invite);
  await expect(viewer).toHaveURL(/\/login\?next=/);
  await viewer.getByRole('link', { name: /Register/ }).click();
  await viewer.getByLabel('First name').fill(vu.first);
  await viewer.getByLabel('Last name').fill(vu.last);
  await viewer.getByLabel('Email').fill(vu.email);
  await viewer.getByLabel('Password').fill(vu.password);
  await viewer.getByRole('button', { name: 'Create account' }).click();
  await finishSignUp(viewer, vu.email, viewer.getByTestId('accept-invite'));
  await viewer.getByTestId('accept-invite').click();
  await expect(viewer.getByTestId('workspace-title')).toBeVisible();
  await viewer.goto(`/browse/${key}-1`);
  const links = viewer.getByTestId('linked-issues');
  await expect(links.getByTestId('links-duplicates')).toContainText('Right');
  await expect(links.getByTestId('add-link')).toHaveCount(0);
  await expect(links.getByRole('button', { name: /Remove link/ })).toHaveCount(
    0,
  );
});

test('linked work items: every relationship is accepted and reads the other way round at the other end', async ({
  page,
}) => {
  await register(page);
  await createWorkspaceSpaceList(page);
  const key = workspaceKey(page);
  await createIssue(page, 'Here');
  await createIssue(page, 'There');
  const [here, there] = [`${key}-1`, `${key}-2`];

  // One link of each type between the same two issues, mixing both directions.
  const made: [relation: string, section: string, other: string][] = [
    ['blocks', 'blocks', 'is_blocked_by'],
    ['is duplicated by', 'is_duplicated_by', 'duplicates'],
    ['is cloned by', 'is_cloned_by', 'clones'],
    ['relates to', 'relates', 'relates'],
  ];
  await page.goto(`/browse/${here}`);
  const links = page.getByTestId('linked-issues');
  for (const [relation, section] of made) {
    await addLink(page, relation, there, there);
    await expect(links.getByTestId('add-link-form')).toBeHidden();
    await expect(links.getByTestId(`links-${section}`)).toContainText(there);
  }
  await page.goto(`/browse/${there}`);
  for (const [, , section] of made)
    await expect(page.getByTestId(`links-${section}`)).toContainText(here);
});
