# Install with Docker

work runs as three containers. Docker Compose starts them together.

| Container  | Image                   | Role                                                                                        |
| ---------- | ----------------------- | ------------------------------------------------------------------------------------------- |
| `frontend` | `eversit/work-frontend` | The web app. The only container published to the host; it proxies `/api/v1` to the backend. |
| `backend`  | `eversit/work-backend`  | The API. Internal network only; runs its database migrations at startup.                    |
| `db`       | `postgres:17`           | PostgreSQL, internal only.                                                                  |

**Requirements:** a Linux host (amd64 or arm64) with Docker Engine 24+ and the Compose plugin, 2 GB of RAM, and a
DNS name with a TLS certificate for anything beyond a trial.

## Images and versions

Both images are public on Docker Hub: [`eversit/work-backend`](https://hub.docker.com/r/eversit/work-backend) and
[`eversit/work-frontend`](https://hub.docker.com/r/eversit/work-frontend). Every release publishes these tags:

| Tag      | Meaning                                                                                 |
| -------- | --------------------------------------------------------------------------------------- |
| `1.4.2`  | Exactly this release. **Use this in production**, so an upgrade is a deliberate change. |
| `1.4`    | The newest patch release of 1.4.                                                        |
| `1`      | The newest release of major version 1.                                                  |
| `latest` | The newest release. Fine for trying it out.                                             |

Always run backend and frontend at the **same** version. What changed is in the
[release notes](https://github.com/EversIT/work/releases).

## 1. Get the compose file

```sh
mkdir work && cd work
curl -fsSLO https://raw.githubusercontent.com/EversIT/work/main/deploy/docker-compose.yml
curl -fsSL  https://raw.githubusercontent.com/EversIT/work/main/deploy/.env.example -o .env
```

## 2. Fill in `.env`

There are deliberately no default secrets: the backend and Compose refuse to start without them. Generate your own:

```sh
openssl rand -hex 24        # POSTGRES_PASSWORD (hex keeps the database URL free of special characters)
openssl rand -base64 48     # WORK_JWT_SECRET (at least 32 bytes)
openssl rand -base64 32     # WORK_ENCRYPTION_KEY (optional: enables two-factor authentication and Slack)
```

The minimum:

```ini
WORK_BACKEND_IMAGE=eversit/work-backend:1.0.0
WORK_FRONTEND_IMAGE=eversit/work-frontend:1.0.0

POSTGRES_USER=work
POSTGRES_DB=work
POSTGRES_PASSWORD=<generated>

WORK_JWT_SECRET=<generated>
WORK_FRONTEND_URL=https://work.example.com
```

::: warning WORK_FRONTEND_URL must be exact
It is the address people type in the browser, used for links in e-mails, cookies and passkeys, including
`https://`. Passkeys only work on a hostname that does not change.
:::

Keep `.env` out of version control and readable only by you (`chmod 600 .env`). Every other variable is in the
[configuration reference](/CONFIGURATION).

## 3. Start

```sh
docker compose up -d
docker compose ps          # all services should be "healthy" or "running"
docker compose logs -f backend
```

The frontend listens on `127.0.0.1:3000`. Put a reverse proxy in front of it (below), or set
`WORK_PUBLISH_ADDR=0.0.0.0` in `.env` to try it from another machine over plain HTTP.

## First sign-in

Open your URL and register. **The first account becomes the instance administrator** and gets the admin panel
(name, accent colour, favicon, users, audit log), so create it yourself right away.

Then decide who may sign up: `WORK_REGISTRATION_ENABLED=false` closes self-registration and you invite people
instead. More administrators:

```sh
docker compose exec backend app make-admin someone@example.com
```

## HTTPS and reverse proxy

Terminate TLS in front of the frontend container. With [Caddy](https://caddyserver.com), which fetches and renews
the certificate itself:

```caddy
work.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

nginx works the same way: proxy to `http://127.0.0.1:3000`, pass `Host`, `X-Forwarded-For` and
`X-Forwarded-Proto`, and switch off response buffering for `/api/v1/` so live updates (server-sent events) arrive
immediately.

::: tip Forwarded addresses
Compose already sets `WORK_TRUST_FORWARDED_FOR=true` for the backend, because its only client is the frontend
container. Make sure your proxy **overwrites** `X-Forwarded-For` instead of passing through what the client sent,
or rate limiting and lock-out can be evaded.
:::

## E-mail

Needed for password reset, invitations, notifications and e-mail confirmation:

```ini
WORK_SMTP_HOST=smtp.example.com
WORK_SMTP_PORT=587
WORK_SMTP_USER=<user>
WORK_SMTP_PASS=<password>
WORK_SMTP_FROM=work <noreply@example.com>   # required as soon as WORK_SMTP_HOST is set
WORK_SMTP_TLS=starttls                      # starttls | tls | none
```

Once SMTP is set, new accounts must confirm their address by default (`WORK_REQUIRE_EMAIL_VERIFICATION`). To test
mail without a real server run Compose with `--profile dev`: it adds Mailpit with a web UI on `127.0.0.1:8025`
(`WORK_SMTP_HOST=mailpit`, port `1025`, TLS `none`).

## File storage

Attachments go to the `uploads` volume by default. For S3 (or a compatible service such as MinIO) or Azure Blob
Storage set `WORK_STORAGE=s3` or `azure` and the matching `WORK_S3_*` / `WORK_AZURE_*` variables. The bucket must
be private and must already exist: files are only ever served through the API, after a permission check.

## Single sign-on

Google, GitHub, Microsoft, Slack, any OpenID Connect provider and SAML 2.0. Register this redirect URI at the
provider and set its client id and secret in `.env`:

```
https://work.example.com/api/v1/auth/sso/<google|github|microsoft|slack>/callback
```

Microsoft needs a pinned tenant (`WORK_SSO_MICROSOFT_TENANT`); `common`, `organizations` and `consumers` are
refused. For SAML, mount the identity provider's metadata into the backend container and point
`WORK_SAML_IDP_METADATA_FILE` at it; the assertion must be signed. Accounts are never merged by e-mail address: an
SSO sign-in creates its own account or signs in to the one that was linked from the account settings.

## Troubleshooting

| Symptom                                                         | Cause and fix                                                                                                            |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `set POSTGRES_PASSWORD` (or similar) when running Compose       | A required value in `.env` is empty. Compose refuses to start rather than use a default.                                 |
| Backend restarts, log says `configuration error: … is required` | Exactly that variable is missing. `WORK_SMTP_FROM` is required as soon as `WORK_SMTP_HOST` is set.                       |
| Sign-in works but the session is lost, or passkeys fail         | `WORK_FRONTEND_URL` does not match the address in the browser (scheme, host or port).                                    |
| Everybody is rate limited at once                               | The proxy does not forward the real client address, so all requests count as one source.                                 |
| Live updates and notifications arrive late                      | The reverse proxy buffers responses. Disable buffering for `/api/v1/`.                                                   |
| Webhooks to an internal address are refused                     | By design: outgoing webhooks may only reach public https hosts. `WORK_WEBHOOK_ALLOW_PRIVATE` / `_ALLOW_HTTP` relax this. |

Health check for monitoring: `GET /api/v1/health`.
