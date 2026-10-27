import { expect, test } from '@playwright/test';
import {
  createProject,
  createWorkspaceSpaceList,
  finishSignUp,
  newUser,
  register,
} from './helpers';

test('a project has its own settings, and someone invited to one project sees only that project', async ({
  browser,
}) => {
  const owner = await (await browser.newContext()).newPage();
  const guest = await (await browser.newContext()).newPage();
  await register(owner);
  await createWorkspaceSpaceList(owner, { ws: 'Acme', list: 'Backlog' });
  await owner.getByTestId('projects-menu').click();
  await owner.getByRole('menuitem', { name: 'Create project' }).click();
  await createProject(owner, 'Other');

  // The settings of the project, not of the whole workspace.
  await owner.getByTestId('projects-menu').click();
  await owner.getByRole('menuitem', { name: /Backlog/ }).click();
  await owner.getByRole('link', { name: 'Project settings' }).click();
  await expect(owner).toHaveURL(/\/lists\/backlog\/settings$/);
  await expect(owner.getByRole('heading', { name: 'Details' })).toBeVisible();
  await owner.getByTestId('ptab-access').click();
  await expect(owner).toHaveURL(/\/lists\/backlog\/settings\/access$/);

  // Visibility can be switched; private projects no longer list "everyone".
  const people = owner.getByTestId('project-access-list');
  await expect(people).toContainText('Everyone in the workspace');
  await owner.getByTestId('visibility-private').click();
  await expect(people).not.toContainText('Everyone in the workspace');
  await owner.getByTestId('visibility-public').click();
  await expect(people).toContainText('Everyone in the workspace');

  // Invite somebody to this one project.
  await owner.getByTestId('invite-to-project').click();
  await owner
    .getByRole('dialog')
    .getByRole('button', { name: 'Create invite' })
    .click();
  const link = await owner.getByTestId('invite-link').inputValue();
  expect(link).toContain('/invite?token=');

  const g = newUser();
  await guest.goto(link);
  await expect(guest).toHaveURL(/\/login\?next=/);
  await guest.getByRole('link', { name: /Register/ }).click();
  await guest.getByLabel('First name').fill(g.first);
  await guest.getByLabel('Last name').fill(g.last);
  await guest.getByLabel('Email').fill(g.email);
  await guest.getByLabel('Password').fill(g.password);
  await guest.getByRole('button', { name: 'Create account' }).click();
  await finishSignUp(guest, g.email, guest.getByTestId('accept-invite'));
  await guest.getByTestId('accept-invite').click();

  // The invitation opens the project itself.
  await expect(guest).toHaveURL(/\/lists\/backlog$/);
  await expect(guest.getByTestId('list-title')).toHaveText('Backlog');

  // Only that project: the other one, although open to the workspace, is not there.
  const workspaceUrl = guest.url().replace(/\/lists\/backlog$/, '');
  await guest.goto(workspaceUrl);
  const projects = guest.getByTestId('project-table');
  await expect(projects.getByText('Backlog', { exact: true })).toBeVisible();
  await expect(projects.getByText('Other', { exact: true })).toHaveCount(0);

  // A member of the project may look at its settings, but not change them.
  await guest.goto(`${workspaceUrl}/lists/backlog/settings`);
  await expect(guest.getByTestId('settings-readonly')).toBeVisible();

  // The owner sees the guest with a "only this project" mark.
  await owner.reload();
  await expect(people).toContainText(g.last);
  await expect(people).toContainText('Only this project');
});
