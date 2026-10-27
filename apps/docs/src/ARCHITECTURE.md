# Architecture

work is a Rust backend, a Next.js frontend and a PostgreSQL database. This page is the map: read it
before your first change, and keep it true when you change the structure.

```
browser ──► Next.js (:3000) ──/api/v1/*──► Rust backend (127.0.0.1:3001) ──► PostgreSQL
              HeroUI v3        proxy          Axum + SQLx                     (+ S3 / Azure Blob, SMTP)
                 ▲                                  │
                 └──── orval-generated client ◄──── OpenAPI document (utoipa)
```

- Only the frontend is published. The backend binds to loopback (or an internal network) and is reached
  through the frontend's `/api/v1` proxy (`apps/frontend/src/proxy.ts`), so the browser only ever talks to
  one origin: no CORS, cookies stay same-site.
- The backend describes itself. `apps/backend/openapi.json` is committed (CI fails when it is stale), and
  `orval` turns it into the typed React Query client in `apps/frontend/src/api/generated` (git-ignored).
  Never write a request by hand when a generated function exists.

## Repository layout

| Path                | What lives there                                                 |
| ------------------- | ---------------------------------------------------------------- |
| `apps/backend`      | the Rust API (see below)                                         |
| `apps/frontend`     | the Next.js app (see below)                                      |
| `apps/frontend-e2e` | Playwright browser tests                                         |
| `deploy/`           | the compose file and `.env.example`                              |
| `packages/docker/`  | Dockerfiles (backend, frontend and demo, docs) and nginx config  |
| `apps/docs/`        | documentation (VitePress site, pages in `src/`)                  |
| `scripts/`          | repository tooling: commit and PR-title lint, publishing helpers |
| `.github/`          | CI workflows, issue and pull request templates                   |

## Backend (`apps/backend/src`)

```
main.rs        process entry: pick the command, start, run
app.rs         the route table, security headers, the server loop
cli.rs         commands besides "serve": openapi, migrate-upstream, make-admin
config/        configuration from the environment (mod.rs, sso.rs, storage.rs)
state.rs       AppState: database, config, limiters, event channel, hub, storage
error.rs       ApiError and how each variant answers
routes/        HTTP handlers, one file per resource (auth, cards, lists, admin, passkeys, ...)
security/      who you are and what you may do
  auth.rs        passwords (Argon2id, bcrypt for imported hashes), JWT, API tokens, the AuthUser extractor
  authz.rs       roles and the one place that decides access to a project, space, list or card
  policy.rs      password policy
  totp.rs        authenticator-app codes and recovery codes
  webauthn.rs    passkey registration and sign-in verification
  ratelimit.rs   fixed-window limiter, in memory or shared through Postgres
  ssrf.rs        which addresses outgoing requests may reach
identity/      signing in with an account that lives elsewhere
  oauth.rs       Google, GitHub, Microsoft, Slack (OAuth 2.0 + PKCE), linking and unlinking
  oidc.rs        generic OpenID Connect: discovery and ID-token verification
  saml.rs        SAML 2.0
domain/        rules that are not tied to one route
  automations/   mod.rs (constants), routes.rs (HTTP), engine.rs (events -> runs -> steps)
  filters.rs     the filter language (a closed AST compiled to bound SQL)
  rich.rs        rich-text documents: what is allowed in one
services/      infrastructure and background work
  events.rs      in-process domain events and the worker that fans them out
  notify.rs      notifications (in-app, e-mail, Slack)
  hub.rs         per-user (and per-document) fan-out for server-sent events
  collab.rs      live collaborative editing of rich-text fields (Yjs)
  scheduler.rs   date-based automation triggers
  storage.rs     uploaded files: local disk, S3, Azure Blob
  mailer.rs      outgoing mail      integrations.rs   Slack
  migrate/       the one-off import from an upstream database
tests/         end-to-end tests against a real Postgres (see DEVELOPMENT.md)
migrations/    SQL migrations, applied at startup
```

### How a request is handled

1. The proxy forwards `/api/v1/...` to the backend. Layers add the security headers, a body limit and a
   timeout.
2. A handler takes an `AuthUser` (extractor in `security/auth.rs`): a session JWT whose session row is still
   live, or a personal API token. Roles are always read from the database, never from the token.
3. The handler asks `security/authz.rs` for the caller's role on the resource's project. Non-members get
   `404`, so ids cannot be probed.
4. It validates the body (explicit structs, never a database row), runs bound-parameter SQL and answers.
5. Changes emit a domain event. The worker turns events into notifications and automation runs, off the
   request path.

### Data model in one paragraph

`project → workspace → space → list`. Users belong to projects with a role (viewer, member, admin, owner);
spaces and lists can be private with explicit grants. A workspace has issue types (`card_types`) and
fields; a card (issue) belongs to a workspace, has a type, `data` JSON keyed by field slug (validated
against the field definitions on every write), and is placed in one or more lists with a stage and an
optional sprint. Issue keys are `PREFIX-n` per workspace.

### Background work

Started by `app::serve`: the event worker, the scheduler (`WORK_SCHEDULER`, safe on several instances at
once through an advisory lock) and, with the shared rate limiter, a purge of old counters. One-off commands
(`migrate-upstream`, `make-admin`) never start the worker, so they send nothing and run no automations.

## Frontend (`apps/frontend/src`)

```
app/              routes (App Router): (app)/ is the signed-in shell, plus login, register, ...
components/       shell/ (top bar, project sidebar, dialogs), views/ (board, list, table, ...),
                  cards/ (issue view), settings/, admin/, rich/ (editor, viewer, live editing)
lib/              pure helpers (theme, grouping, rich-text, passkeys, csp) - unit tested
api/              fetch wrapper (token refresh) and the generated client
proxy.ts          forwards /api/v1 and sets the per-request Content-Security-Policy nonce
messages/         en.json, de.json
```

- **UI kit.** HeroUI v3 components and its design tokens (`bg-accent`, `text-muted`, `border-border`, ...).
  Do not hard-code colours: the administrator's accent colour is applied by overriding these tokens in
  `app/layout.tsx` (`lib/theme.ts`), and a new theme is just another set of token values.
- **Branding.** Title, description, favicon and accent come from `GET /api/v1/branding`, read on the server
  for every page (`lib/branding.ts`, `generateMetadata`).
- **Navigation.** Jira-Cloud style: a top bar everywhere, a project sidebar only inside a project, projects
  created in one step (a space is made on the fly).
- **Auth in the browser.** The access token lives in memory only; the refresh token is an `HttpOnly` cookie
  (`api/fetcher.ts`).
- **CSP.** `script-src` uses a per-request nonce (`proxy.ts`, `lib/csp.ts`); pages are rendered per request.
- **Rich text** is a ProseMirror document (JSON) rendered by React components, never as HTML. Descriptions
  are edited live by several people through Yjs (`components/rich/collab-*.ts(x)`, backend `services/collab.rs`).

## Tests

| Layer               | Where                            | Needs                                           |
| ------------------- | -------------------------------- | ----------------------------------------------- |
| Rust unit tests     | next to the code                 | nothing                                         |
| Rust end-to-end     | `apps/backend/src/tests/`        | PostgreSQL; optionally Mailpit, S3Mock, Azurite |
| Frontend unit tests | `apps/frontend/src/**/*.test.ts` | nothing                                         |
| Browser end-to-end  | `apps/frontend-e2e/src/`         | the running stack (+ Mailpit)                   |

## Known limitations

- Realtime fan-out (notifications, live editing) is in-process. With several backend instances, route one
  card's editors to the same instance; saved content is shared through the database either way.
- The default rate limiter counts per instance; `WORK_RATE_LIMIT_BACKEND=postgres` shares it.
- The SAML implementation is a pre-release crate (`saml` 0.1 alpha) without an independent audit; single
  logout and encrypted assertions are not wired up. SAML needs https (the response arrives as a cross-site POST).
- Sign-in through a provider (SSO, SAML) does not ask for the local second factor; the provider is
  responsible for authenticating the person. Passkeys count as two factors.
- Attestation of passkeys is not verified ("none"); only ES256 credentials are supported.
- Slack and SMTP delivery were exercised against test doubles (Mailpit for SMTP); the Slack HTTP exchange was never run against Slack itself.
- SSO, SAML, S3 and Azure are tested against local stand-ins (a mock provider, a SAML IdP built with the
  same library, S3Mock, Azurite), not against the real services.
- The upstream import works through an export contract (`apps/docs/src/DATA-MIGRATION.md`); the views mapping a
  particular upstream deployment onto it are the operator's to write.
