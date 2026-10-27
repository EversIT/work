# work

An open-source, self-hosted work-management tool in the spirit of Jira Cloud: projects with boards, backlog
and sprints, issue keys (`WORK-12`), issue types, priorities, story points, rich-text descriptions that several
people edit at once, automations with webhooks, and a documented API with personal API tokens.

A **Rust** backend (Axum, SQLx, PostgreSQL) and a **Next.js** frontend on **HeroUI v3**.

## Features

- **Projects** (Kanban or Scrum), created in one step, grouped in spaces (public or private with per-user grants)
  inside workspaces with an issue key prefix. Roles per project: viewer, member, admin, owner.
- **Board, backlog, list, table, calendar and timeline.** Drag and drop between statuses and sprints; group list
  and table by status or any field.
- **Issues** with type, summary, rich-text description (live editing by several people), assignee, reporter,
  priority, labels, story points, dates, child issues, comments with formatting and attachments, history,
  watchers and notifications (live, e-mail, Slack). Custom issue types and fields, and contact, company and
  deal layouts for light CRM use.
- **Search and filters** from the top bar, quick filters and saved filters per view.
- **Automations:** triggers (issue created, field changed, status changed, comment, assignee changed, a date
  arrives, incoming webhook) and actions (set field, move, assign, comment, create issue, send webhook, send e-mail).
- **API:** everything the UI does is a documented REST call. Use `Authorization: Bearer wrk_…` or, Jira-style,
  Basic `email:token`. The OpenAPI document is at `/api/v1/openapi.json`.
- **Sign-in:** password with a policy and lock-out, **passkeys (WebAuthn)**, **authenticator-app codes**,
  e-mail confirmation, and single sign-on with **Google, GitHub, Microsoft, Slack, any OpenID Connect provider
  and SAML 2.0**.
- **Administration:** an admin panel for the instance's name, description, favicon and accent colour, an overview
  of users, projects and settings, user management and an audit log.
- **Files** on local disk, S3 (or compatible) or Azure Blob Storage.
- **English and German.**

## Quick start

Requirements: Node 24+, Rust (via [rustup](https://rustup.rs)), PostgreSQL 15+, Docker for images and tests.

```sh
npm ci
cp deploy/.env.example .env     # fill in the required values; there are no defaults for secrets
npx nx run backend:serve        # needs the WORK_* variables from .env in its environment
npx nx run frontend:dev
```

The first account you create is the instance administrator.

With Docker:

```sh
npx nx run-many -t container:local
docker compose -f deploy/docker-compose.yml --env-file .env up
```

Compose refuses to start when a secret is missing and publishes only `127.0.0.1:3000`; put a TLS-terminating
reverse proxy in front for production and set `WORK_FRONTEND_URL` to the public `https://` address.

## Documentation

|                                                                    |                                                         |
| ------------------------------------------------------------------ | ------------------------------------------------------- |
| [apps/docs/src/DEVELOPMENT.md](apps/docs/src/DEVELOPMENT.md)       | set up, tests, conventions, recipes                     |
| [apps/docs/src/ARCHITECTURE.md](apps/docs/src/ARCHITECTURE.md)     | how the code is organised and why                       |
| [apps/docs/src/CONFIGURATION.md](apps/docs/src/CONFIGURATION.md)   | every environment variable                              |
| [apps/docs/src/SECURITY-MODEL.md](apps/docs/src/SECURITY-MODEL.md) | how it is protected, and what is left                   |
| [apps/docs/src/DATA-MIGRATION.md](apps/docs/src/DATA-MIGRATION.md) | importing from the NestJS/Vue project this derives from |
| [CONTRIBUTING.md](CONTRIBUTING.md)                                 | how to contribute                                       |
| [SECURITY.md](SECURITY.md)                                         | how to report a vulnerability                           |

## Using the API

```sh
curl -H "Authorization: Bearer wrk_…" https://work.example.com/api/v1/me
curl -u you@example.com:wrk_… https://work.example.com/api/v1/projects
curl -X POST -H "Authorization: Bearer wrk_…" -H "content-type: application/json" \
     -d '{"data":{"title":"Created through the API"}}' https://work.example.com/api/v1/lists/<list-id>/cards
```

Tokens are read-only or read-write, can expire, act with your permissions and can be revoked at any time.
Managing tokens, passwords and the account itself needs a browser session.

## Publishing images and releases

Images are published to Docker Hub as `eversit/work-backend`, `eversit/work-frontend`, `eversit/work-docs` (the
documentation site) and `eversit/work-demo` (the demo, see below). Run the **release**
workflow (Actions, "Run workflow" on `main`): `nx release` derives the next version from the conventional
commits, updates `CHANGELOG.md`, tags `vX.Y.Z`, creates the GitHub release, and the **publish** workflow pushes
the images as `X.Y.Z`, `X.Y`, `X` and `latest`. The first run needs "first release" ticked.

One-time setup: create the four public repositories on Docker Hub, and add the repository secrets
`DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` (an access token with Read & Write). To publish somewhere else, set the
repository variables `IMAGE_REGISTRY` and `IMAGE_NAMESPACE`. The installation guide is in
[apps/docs](apps/docs/src) (VitePress) and can be published to GitHub Pages by the **docs** workflow or run as its own
image, `eversit/work-docs`.

## Documentation site and demo

- **Docs:** `apps/docs` is a [VitePress](https://vitepress.dev) site (pages in `apps/docs/src`).
  `npx nx run docs:dev` to write, `npx nx run docs:build` to build into `dist/apps/docs`, and the `work-docs` image
  serves it with nginx.
- **Demo:** `apps/demo` builds the frontend with `NEXT_PUBLIC_DEMO=true`. Every API call is then answered in the browser
  from made-up data (`apps/frontend/src/demo`): no backend, no database, nothing is stored, and a reload starts over.
  `npx nx run demo:dev` (port 3100) or `docker run -p 3000:3000 eversit/work-demo`.

## License

MIT, see [LICENSE](LICENSE). The license keeps the copyright notice of the original project this code derives
from, as the MIT license requires.
