# work

An open-source, self-hosted alternative to Jira: projects with boards, backlog and sprints, issue keys (`WORK-12`),
issue types, priorities, story points, automations with webhooks, and an API with personal API tokens.

Built with a **Rust** backend (Axum, SQLx, PostgreSQL) and a **Next.js** frontend on **HeroUI v3**. It started as a
port of an existing NestJS/Vue project; the security problems found in that code were designed out
(see [docs/SECURITY-AUDIT.md](docs/SECURITY-AUDIT.md)).

## What you get

- **Projects** (Kanban or Scrum) in **spaces**, public or private with per-user grants, inside **workspaces** with an
  issue key prefix. Roles per project: viewer, member, admin, owner.
- **Board, backlog, list, calendar and timeline.** Drag and drop between statuses and sprints; start and complete sprints.
- **Issues** with type (Epic, Story, Task, Bug, Sub-task), summary, description, assignee, reporter, priority, labels,
  story points, dates, child issues, comments with attachments, history, watchers and notifications (in-app live via
  SSE, e-mail, Slack). Custom issue types and fields.
- **Search and filters:** by key or text from the top bar, quick filters, saved filters per view.
- **Automations:** triggers (issue created, field changed, status changed, comment, assignee changed, **incoming
  webhook**) and actions (set field, move, assign, comment, create issue or sub-task, **send webhook**, **send e-mail**).
- **API:** everything the UI does is a documented REST call. Create **API tokens** in your account settings and use
  them as `Authorization: Bearer wrk_…` or, Jira-style, with Basic auth `email:token`. The OpenAPI document is served at
  `/api/v1/openapi.json`.
- **English and German.** The language is stored in a cookie, never in the URL.

## Architecture

```
browser ──► Next.js (:3000) ──/api/v1/*──► Rust backend (127.0.0.1:3001) ──► PostgreSQL
              HeroUI v3        proxy          Axum + SQLx
                 ▲                                │
                 └──── orval-generated client ◄── OpenAPI (utoipa)
```

- Only the frontend port is exposed. The backend is reached through the Next proxy (`apps/frontend/src/proxy.ts`).
- The backend generates its OpenAPI document. `apps/backend/openapi.json` is committed, CI fails if it is stale, and
  `orval` generates the typed React Query client from it into `apps/frontend/src/api/generated` (git-ignored).
- Rust is wired into Nx with [`@monodon/rust`](https://nx.dev/docs/kb/add-rust-to-nx-workspace); `apps/backend` is a normal Nx project.

## Develop

Requirements: Node 24+, Rust via [rustup](https://rustup.rs) (toolchain pinned in `rust-toolchain.toml`), PostgreSQL 15+,
Docker (for the image targets and tests).

```sh
cp .env.example .env        # fill in every required value; there are no defaults for secrets
npm ci
npx nx run backend:serve    # needs the WORK_* variables from .env in the environment
npx nx run frontend:dev     # regenerates the API client first
```

Useful targets: `npx nx run backend:test`, `backend:lint`, `backend:openapi`, `frontend:orval`, `frontend:build`,
`frontend-e2e:e2e`.

### Tests

| What | How |
|------|-----|
| Unit tests | `npx nx run backend:test` |
| Database end-to-end (authorization, SSRF, webhooks, sprints, …) | `DATABASE_URL=postgres://… npx nx run backend:test --args=--include-ignored` |
| Mail flows (password reset, invitations, automation mail) | additionally `TEST_SMTP_PORT=… TEST_MAILPIT_API=http://…` pointing at a [Mailpit](https://mailpit.axllent.org) |
| Browser end-to-end (Playwright) | start backend + frontend (or the compose stack), then `PW_CHANNEL=chrome E2E_BASE_URL=http://localhost:3000 npx nx run frontend-e2e:e2e` |

## Docker images

Nx builds both images (`@nx-tools/nx-container`), packaging prebuilt artifacts:

| Image | Dockerfile | Built from |
|-------|------------|------------|
| `backend` | `rust.Dockerfile` (ubuntu base, non-root, linux/amd64 + linux/arm64) | `backend:build`, `backend:build-arm64` |
| `frontend` | `next.Dockerfile` | `frontend:build` (Next standalone output) |

```sh
npx nx run-many -t container:local      # builds work-backend:latest and work-frontend:latest
```

**Publishing** is the `publish` GitHub Actions workflow (manual dispatch or a published release). It pushes
`ghcr.io/<owner>/backend` and `ghcr.io/<owner>/frontend` with the built-in `GITHUB_TOKEN`; nothing to configure.
To move to **Docker Hub** later, set the repository variables `IMAGE_REGISTRY=docker.io` and
`IMAGE_NAMESPACE=<hub user or org>` and add the `DOCKERHUB_USERNAME` / `DOCKERHUB_TOKEN` secrets. The workflow itself
does not change.

## Run with Docker

```sh
cp .env.example .env   # fill in the required values, including the two image refs
docker compose up
```

Compose refuses to start when a secret is missing. Only `127.0.0.1:3000` is published. Put a TLS-terminating reverse proxy
in front for production and set `WORK_FRONTEND_URL` to the public `https://` URL. `docker compose --profile dev up` adds
Mailpit (web UI on `127.0.0.1:8025`) for trying e-mail features.

## Using the API

```sh
curl -H "Authorization: Bearer wrk_…" https://work.example.com/api/v1/me
curl -u you@example.com:wrk_… https://work.example.com/api/v1/projects
curl -X POST -H "Authorization: Bearer wrk_…" -H "content-type: application/json" \
     -d '{"data":{"title":"Created through the API"}}' https://work.example.com/api/v1/lists/<list-id>/cards
```

Tokens are read-only or read-write, can expire, act with your permissions and can be revoked at any time.
Managing tokens, passwords and the account itself needs a browser session.

## License

MIT, see [LICENSE](LICENSE). The license keeps the copyright notice of the original project this code derives from, as the MIT license requires.
