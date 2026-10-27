# Importing data from the upstream NestJS/Vue database

The rewrite does not share a schema with the upstream project (UUIDs instead of integer ids, project →
workspace → space → list instead of a per-resource ACL table), and the upstream schema differs between
deployments (79 migrations). The importer therefore does not read upstream tables. It reads an **export
contract**: twelve views that you create once in the source database (or in a restored copy of it) and that
map your tables onto fixed columns. Writing them is the only part that needs knowledge of your upstream
deployment; everything after it is automatic and tested.

```
WORK_MIGRATE_SOURCE_URL=postgres://reader@old-db/upstream \
WORK_DATABASE_URL=... WORK_JWT_SECRET=... WORK_FRONTEND_URL=... \
work-backend migrate-upstream --dry-run            # reads and checks, writes nothing
work-backend migrate-upstream [--schema work_export]
```

The target server must have run its migrations (start it once). Use read-only access to the source. The
exit status is `0` for a clean import, `1` when the report lists problems (the import still went through),
`2` when it could not run.

## The contract

All views live in one schema (default `work_export`). Ids can be integers, UUIDs or text; they are only used
to relate rows. Columns that may be `NULL` are marked.

| View              | Columns                                                                                                                                         |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `users`           | `id`, `email`, `first_name`, `last_name`, `password_hash` (bcrypt or Argon2; anything else means "no password"), `created_at`                   |
| `projects`        | `id`, `name`                                                                                                                                    |
| `project_members` | `project_id`, `user_id`, `role` (`owner`, `admin`, `member`, `viewer`; `guest`, `user`, `editor` are understood)                                |
| `workspaces`      | `id`, `project_id`, `name`, `key_prefix` (nullable; 2-10 capital letters/digits, else derived from the name)                                    |
| `spaces`          | `id`, `workspace_id`, `name`, `is_private`                                                                                                      |
| `lists`           | `id`, `space_id`, `name`, `methodology` (`kanban`/`scrum`, nullable)                                                                            |
| `stages`          | `id`, `list_id`, `name`, `sort_order`, `is_completed`                                                                                           |
| `card_types`      | `id`, `workspace_id`, `name`, `layout` (`default`/`person`/`organization`/`deal`, nullable)                                                     |
| `fields`          | `id`, `workspace_id`, `card_type_id` (nullable = workspace-wide), `name`, `slug`, `type`, `items` (nullable JSON array), `multiple`, `required` |
| `cards`           | `id`, `workspace_id`, `card_type_id`, `parent_id` (nullable), `data` (JSON keyed by field slug), `created_by` (nullable), `created_at`          |
| `card_lists`      | `card_id`, `list_id`, `stage_id` (nullable = first stage)                                                                                       |
| `comments`        | `id`, `card_id`, `user_id` (nullable), `content` (text), `created_at`                                                                           |

`type` is one of `text`, `text_area`, `dropdown`, `label`, `user`, `date`, `date_time`, `number`,
`percentage`, `checkbox`, `email`, `url`, `currency`, `rich`, `card`. In `data`, user fields (such as the assignee) hold **source user ids** (a value or a list); the importer
translates them to the imported users and reports the ones it cannot match. Dates are ISO 8601.

Sketch of one view (your table and column names will differ):

```sql
CREATE SCHEMA work_export;
CREATE VIEW work_export.users AS
  SELECT u.id, u.email, u."firstName" AS first_name, u."lastName" AS last_name,
         u.password AS password_hash, u."createdAt" AS created_at
    FROM public."user" u;
```

## What the import does

- **Users, projects, memberships** are written directly. A user whose address already has an account here is
  mapped to it and left unchanged. Every project gets an owner (the first admin or member is promoted when the
  source has none). Projects without any importable member are skipped with everything in them.
- **Everything else goes through this server's own API**, in process, acting as the project owner or - for cards
  and comments - as the original author when they may still write (viewers' items are attributed to the owner).
  So field validation, issue keys (`KEY-1`, `KEY-2`, ... in the original creation order), the default issue types
  (Task, Epic, Story, Bug, Sub-task), views and stages are exactly what a user would get. The original
  `created_at` of users, cards and comments is restored afterwards.
- A list's three default stages are replaced by the source's stages when the source has any. A card in several
  lists stays in all of them.
- **Idempotent.** Every created object is recorded in `migration_ids`; a second run skips what exists and
  retries what failed. You can fix the views and run again.
- **Nothing is dropped silently.** The report lists every row that was skipped and every value that had to be
  left out for the card to be created (unknown field keys, values the field refuses).
- **Passwords:** bcrypt hashes are kept and verify as before; they are replaced by Argon2id at the next sign-in.
  Users without a usable hash use "forgot password".
- **No side effects:** no notifications, e-mails or automations run, because the event worker is not started.

## What is not imported

Uploaded files (their bytes usually are not in the database; upload them again or script it against the files
API), automations, notifications and watchers, Slack connections, invites, API tokens, sessions, per-resource ACL
entries (roles come from `project_members`; make spaces `is_private` and grant access in the UI), upstream CRM
views, and rich-text structure inside `comments.content` (it arrives as text).
