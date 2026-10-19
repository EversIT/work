# Migration status: NestJS/Vue → Rust/Next.js

Source: the upstream NestJS/Vue project (≈16k lines backend, ≈32k lines frontend, 79 TypeORM migrations).
The schema is **not** carried over 1:1: ids are UUIDs, tenancy is `project → workspace → space → list`, and
authorization is a project role plus grants on private spaces and lists instead of the per-resource ACL table.
Existing upstream databases therefore need a one-off data migration script (not written).

The product direction also changed from a generic work manager to a **Jira alternative**: issue keys, issue types,
priority, story points, sprints and a backlog, Jira-style navigation and issue view, API tokens, webhooks.

## Ported (backend + frontend, with tests)

| Area | Notes |
|------|-------|
| Auth, sessions, refresh rotation, password change, account deletion | plus password reset by e-mail and personal API tokens |
| Projects, members, roles, invites (link and e-mail) | invites are hashed, expiring, use-limited, optionally bound to an address |
| Workspaces, spaces, lists/projects, stages (statuses), views | Kanban and Scrum projects; board, list, table, calendar, timeline |
| Issue types and custom fields | validated against the field definition on every write |
| Cards/issues, children, search, filters, sorting, pagination | filter engine with allow-listed columns, bound parameters |
| Backlog and sprints | plan, start, complete, move unfinished issues |
| Activities: comments, tasks, history | attachments through the files API |
| Notifications and watchers | in-app (SSE), e-mail, Slack |
| Files | local disk storage with sniffing, quota and safe serving |
| Automations | all upstream triggers/actions except the date based ones, plus webhooks and e-mail |
| Slack integration | OAuth with server-side state, encrypted tokens (needs a real Slack app to run) |
| Mailer | SMTP, no public send endpoint, escaped templates |
| i18n | English and German |

## Not ported

| Upstream feature | Status |
|------------------|--------|
| S3 and Azure file storage | local disk only |
| Date-based automation triggers (due/start date arrives) | needs a scheduler |
| TipTap rich text and real-time collaborative editing | descriptions and comments are plain text |
| CRM layouts (contacts, organizations, deals) | layouts are stored but have no dedicated UI |
| List groups UI | the API exists (`/lists/{id}/groups`), views do not group yet |
| Data migration from an upstream database | not written |

## Known gaps in what exists

- Slack and SMTP delivery were exercised against test doubles (Mailpit for SMTP); the Slack HTTP exchange was not run against Slack.
- Rate limiting is in-process (per instance).
- CSP still allows `'unsafe-inline'` scripts (Next bootstrap). Next step: per-request nonce.
- Registration reveals whether an e-mail exists (`409`); a proper fix needs e-mail verification at sign-up.
- No password policy beyond length, no MFA.
