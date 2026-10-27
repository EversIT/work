import { expect, test } from '@playwright/test';
import {
  createIssue,
  createWorkspaceSpaceList,
  finishSignUp,
  newUser,
  register,
} from './helpers';

test('invite by link, assignment notifies in realtime, private spaces stay hidden until granted', async ({
  browser,
}) => {
  const owner = await (await browser.newContext()).newPage();
  const member = await (await browser.newContext()).newPage();
  const ou = await register(owner);
  await createWorkspaceSpaceList(owner);

  // Owner creates an issue and a private space with a project in it.
  await createIssue(owner, 'Shared task');
  await expect(owner.getByTestId('card-Shared task')).toBeVisible();

  // A private space, created in the workspace settings, with a project in it.
  await owner.getByTestId('workspace-settings-link').click();
  await owner.getByTestId('tab-spaces').click();
  await owner.getByTestId('new-space').click();
  const d = owner.getByRole('dialog');
  await d.getByLabel('Name').fill('Secret');
  await d.getByText('Private space').click();
  await d.getByRole('button', { name: 'Create' }).click();
  await expect(
    owner.getByText('Secret', { exact: true }).first(),
  ).toBeVisible();
  await owner.getByTestId('projects-menu').click();
  await owner.getByRole('menuitem', { name: 'Create project' }).click();
  await owner
    .getByRole('dialog')
    .getByLabel('Project name')
    .fill('Hidden list');
  await owner.getByTestId('project-space').click();
  await owner.getByRole('option', { name: 'Secret' }).click();
  await owner
    .getByRole('dialog')
    .getByRole('button', { name: 'Create' })
    .click();
  await expect(owner.getByTestId('list-title')).toHaveText('Hidden list');

  // Invite link from the project settings.
  await owner.getByTestId('workspace-settings-link').click();
  await owner.getByTestId('tab-people').click();
  await owner.getByTestId('invite').click();
  await owner
    .getByRole('dialog')
    .getByRole('button', { name: 'Create invite' })
    .click();
  const link = await owner.getByTestId('invite-link').inputValue();
  expect(link).toContain('/invite?token=');
  await owner
    .getByRole('dialog')
    .getByRole('button', { name: 'Close' })
    .last()
    .click();

  // The invitee registers through the invite link and joins as member.
  const mu = newUser();
  await member.goto(link);
  await expect(member).toHaveURL(/\/login\?next=/);
  await member.getByRole('link', { name: /Register/ }).click();
  await member.getByLabel('First name').fill(mu.first);
  await member.getByLabel('Last name').fill(mu.last);
  await member.getByLabel('Email').fill(mu.email);
  await member.getByLabel('Password').fill(mu.password);
  await member.getByRole('button', { name: 'Create account' }).click();
  await finishSignUp(member, mu.email, member.getByTestId('accept-invite')); // `next` survives the registration (and the confirmation mail)
  await member.getByTestId('accept-invite').click();
  await expect(member.getByTestId('workspace-title')).toHaveText('Acme');
  // The private space and its project are not visible to the member.
  const projects = member.getByTestId('project-table');
  await expect(projects.getByText('Backlog', { exact: true })).toBeVisible();
  await expect(projects.getByText('Hidden list', { exact: true })).toHaveCount(
    0,
  );
  await expect(projects.getByText('Secret', { exact: true })).toHaveCount(0);

  // Owner assigns the member to the card: the member gets a live toast and a bell badge.
  await owner.reload(); // the member list refreshes on load (and every 30 seconds)
  await owner.getByTestId('projects-menu').click();
  await owner.getByRole('menuitem', { name: /Backlog/ }).click();
  await owner.getByTestId('card-Shared task').click();
  const detail = owner.getByTestId('issue-dialog');
  await detail.getByRole('button', { name: /Assignee/ }).click();
  await owner.getByRole('option', { name: new RegExp(mu.last) }).click();
  await expect(member.getByTestId('unread-count')).toHaveText('1', {
    timeout: 15_000,
  });
  await member.getByTestId('bell').click();
  await expect(member.getByTestId('notification-list')).toContainText(
    'assigned you to "Shared task"',
  );
  await member.getByTestId('mark-all').click();
  await expect(member.getByTestId('unread-count')).toHaveCount(0);

  void ou;
});
