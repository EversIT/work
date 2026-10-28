# Development

## Set up

Requirements: Node 24+, Rust through [rustup](https://rustup.rs) (the toolchain is pinned in
`rust-toolchain.toml`), PostgreSQL 15+ (`citext` extension), Docker for images and the test emulators.

```sh
npm ci
cp packages/deploy/.env.example .env     # fill in the required values; there are no defaults for secrets
npx nx run backend:serve        # the backend needs the WORK_* variables from .env in its environment
npx nx run frontend:dev         # regenerates the API client first, then starts Next.js
```

A quick way to get PostgreSQL: `docker run -d -p 5432:5432 -e POSTGRES_HOST_AUTH_METHOD=trust postgres:17-alpine`
(local development only; `trust` means no password).

Everything goes through Nx: `npx nx run <project>:<target>`. Useful targets: `backend:serve`, `backend:test`,
`backend:lint`, `backend:openapi`, `frontend:dev`, `frontend:build`, `frontend:lint`, `frontend:test`,
`frontend:orval`, `frontend:api`, `frontend-e2e:e2e`.

## Tests

| What                              | Command                                                                                                                                    |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust unit tests                   | `npx nx run backend:test`                                                                                                                  |
| Rust end-to-end (real PostgreSQL) | `DATABASE_URL=postgres://… npx nx run backend:test --args=--include-ignored`                                                               |
| Frontend unit tests               | `npx nx run frontend:test`                                                                                                                 |
| Browser end-to-end                | start backend and frontend (or the compose stack), then `PW_CHANNEL=chrome E2E_BASE_URL=http://localhost:3000 npx nx run frontend-e2e:e2e` |

The end-to-end tests marked `#[ignore]` need `DATABASE_URL`; `#[sqlx::test]` creates a throw-away database
for each test. Some also use local emulators and **skip themselves** when they are not configured:

| Emulator                               | Used by       | Start                                                                                                                                                                        | Variables                                                                  |
| -------------------------------------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| [Mailpit](https://mailpit.axllent.org) | mail flows    | `docker run -d -p 1025:1025 -p 8025:8025 axllent/mailpit`                                                                                                                    | `TEST_SMTP_PORT=1025`, `TEST_MAILPIT_API=http://127.0.0.1:8025`            |
| S3Mock                                 | S3 storage    | `docker run -d -p 9090:9090 -e COM_ADOBE_TESTING_S3MOCK_STORE_INITIAL_BUCKETS=work-test adobe/s3mock`                                                                        | `TEST_S3_ENDPOINT=http://127.0.0.1:9090`, `TEST_S3_BUCKET=work-test`       |
| Azurite                                | Azure storage | `docker run -d -p 10000:10000 -e AZURITE_ACCOUNTS="work:$KEY" mcr.microsoft.com/azure-storage/azurite azurite-blob --blobHost 0.0.0.0` with `KEY=$(openssl rand -base64 32)` | `TEST_AZURE_ENDPOINT=http://127.0.0.1:10000`, `TEST_AZURE_ACCESS_KEY=$KEY` |

Test credentials are made up per run (`openssl rand`); never commit one.

The browser tests expect a stack with SMTP (for confirmation mails; Mailpit's web API at `E2E_MAILPIT_API`,
default `http://127.0.0.1:58025`) and `WORK_ENCRYPTION_KEY` (for two-factor authentication). Run against a
fresh database: the first account becomes the administrator, which `admin.spec.ts` relies on.

## Conventions

- **Commits and pull request titles:** `type(scope): subject`, enforced by `scripts/commit-lint.js`.
  Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, ... Scopes: `core`, `repo`, `docs`, `deps`, and the
  folder names under `apps/` (`backend`, `frontend`, `frontend-e2e`).
- **Rust:** `cargo fmt` and `clippy -D warnings` must pass (`backend:lint`). Comment the _why_; the code says
  what. Errors go through `ApiError`; handlers return `ApiResult`.
- **TypeScript:** `frontend:lint` must pass with no errors. Use HeroUI components and design tokens, never
  hard-coded colours. Strings go through `next-intl` (`messages/en.json` and `de.json`, same keys in both).
- **Prettier** formats staged files on commit (husky).

## Recipes

**Add an endpoint**

1. Put the handler in `apps/backend/src/routes/<resource>.rs`. Take `AuthUser`, call `security::authz` for the
   resource's project, use an explicit request struct, bound SQL parameters.
2. Annotate it with `#[utoipa::path(...)]` (give it an `operation_id`) and register it with `routes!(...)`.
3. Register the router in `app.rs` if it is a new file.
4. `npx nx run frontend:api` (it runs `backend:openapi`, then `frontend:orval`); commit `apps/backend/openapi.json`. `orval` alone reads the committed document and needs no Rust, which is what CI and the image builds use.
5. Add an end-to-end test in `apps/backend/src/tests/` that covers who must **not** be allowed, not only the happy path.

**Add a migration**: a new `apps/backend/migrations/NNNN_name.sql` (never edit an applied one). Migrations
run at startup and in every `#[sqlx::test]`.

**Add a setting**: read it in `config/`, document it in `apps/docs/src/CONFIGURATION.md` and `packages/deploy/.env.example`,
pass it through `packages/deploy/docker-compose.yml`. Never give a secret a default.

**Add a translation key**: add it to both `messages/en.json` and `messages/de.json`.

**Change a theme token or the look**: use tokens (`bg-accent`, `text-muted`, ...); `lib/theme.ts` maps the
administrator's accent colour onto them.

## Running the whole stack in Docker

```sh
npx nx run-many -t container:local                        # work-backend:latest, work-frontend:latest
cp packages/deploy/.env.example .env                               # fill in the required values
docker compose -f packages/deploy/docker-compose.yml --env-file .env up
```

Compose refuses to start when a secret is missing. Only `127.0.0.1:3000` is published; put a TLS-terminating
reverse proxy in front for production. `--profile dev` adds Mailpit.
