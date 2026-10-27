# People and access

work has three levels. Knowing which one a setting lives on explains where to find it.

| Level         | What it is                                                           | Where its settings are                            |
| ------------- | -------------------------------------------------------------------- | ------------------------------------------------- |
| **Workspace** | The home of a team: its people, issue types, spaces and automations. | Gear icon in the top bar → **Workspace settings** |
| **Space**     | A folder of projects. It can be private.                             | Workspace settings → **Spaces**                   |
| **Project**   | A board or a backlog: its statuses, fields and who may open it.      | Sidebar of the project → **Project settings**     |

## Roles

The same four roles are used everywhere:

| Role       | May                                               |
| ---------- | ------------------------------------------------- |
| **Viewer** | Look at everything they can open, change nothing. |
| **Member** | Create and edit issues, comment.                  |
| **Admin**  | Also manage people, settings and automations.     |
| **Owner**  | Everything, including deleting the workspace.     |

## Who can see a project

- A project is **open to the workspace** or **private**. Open projects can be opened by everybody in the workspace,
  private ones only by the people added in the project's **Access** settings and by workspace admins.
- A private **space** hides everything inside it in the same way.
- A person with **limited access** is a member of the workspace who sees _no_ project unless they were added to it,
  not even the open ones. That is the right role for guests and customers.

## Adding someone to one project only

Open the project's **Project settings → Access** and choose **Invite**:

1. Create a link, or enter an e-mail address (the link then only works for that address).
2. The person registers or signs in and accepts the invitation.
3. They are now in the workspace with **limited access** and see exactly this project, with the role you chose.

Someone who is already in the workspace is simply added to the project with **Add a person from the workspace**.
Project admins can do this for their own project; workspace admins can do it for any.

To let a workspace member see only some projects, switch on **Limited access** for them under
**Workspace settings → People** and add them to those projects.

## Automation

Rules do work when something happens: _when_ an issue is created, a status changes or a date arrives, _then_ move,
assign, comment, call a webhook or send an e-mail.

- Rules of **one project** are in **Project settings → Automation**. They run only for issues of that project.
- Rules for the **whole workspace** are in **Workspace settings → Automations**. Each rule shows where it applies.
- A rule is built on its own page. Take actions from the palette on the left and drop them on the line where they
  should happen, drag the handle of an action to change the order, and click any box to set it up. The buttons
  _Move up_ and _Move down_ do the same without a mouse.
- Rules act without the permission checks of single projects, so only workspace admins can create or change them.
