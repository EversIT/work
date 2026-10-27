# Configuration

Everything is configured through environment variables (`deploy/.env.example` is the annotated template).
**No secret has a default**: a missing or weak secret stops the server at startup. Settings that are not
secret can be changed later in the admin panel where noted.

## Required

| Variable            | Meaning                                                                                                                                             |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WORK_DATABASE_URL` | PostgreSQL connection string. Migrations run at startup.                                                                                            |
| `WORK_JWT_SECRET`   | At least 32 bytes (`openssl rand -base64 48`). Signs session tokens, derives other keys.                                                            |
| `WORK_FRONTEND_URL` | The public URL users open. `https://` in production (`http://localhost` is accepted for development). Used for links in mail, cookies and passkeys. |

## Server

| Variable                    | Default                 | Meaning                                                                                  |
| --------------------------- | ----------------------- | ---------------------------------------------------------------------------------------- |
| `WORK_BIND_ADDR`            | `127.0.0.1:3001`        | Where the backend listens. Do not publish it; the frontend proxies to it.                |
| `WORK_TRUST_FORWARDED_FOR`  | `false`                 | Use the last `X-Forwarded-For` entry as client address. Only behind a proxy you control. |
| `WORK_REGISTRATION_ENABLED` | `true`                  | Close self-registration once your accounts exist.                                        |
| `WORK_AUTH_RATE_LIMIT`      | `10`                    | Sign-in and similar attempts per address and minute.                                     |
| `WORK_RATE_LIMIT_BACKEND`   | `memory`                | `postgres` shares the counters between several instances.                                |
| `WORK_SCHEDULER`            | `true`                  | Run date-based automation triggers in this process. Safe on several instances.           |
| `BACKEND_URL` (frontend)    | `http://localhost:3001` | Where the Next.js proxy forwards `/api/v1`.                                              |

## Accounts and sign-in

| Variable                                                                         | Meaning                                                                                                                                                             |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WORK_ENCRYPTION_KEY`                                                            | 32 bytes, base64 (`openssl rand -base64 32`). Encrypts stored tokens and authenticator secrets. **Without it two-factor authentication and Slack are unavailable.** |
| `WORK_REQUIRE_EMAIL_VERIFICATION`                                                | Sign-ups must confirm their address. Unset = on whenever SMTP is configured.                                                                                        |
| `WORK_WEBAUTHN_RP_ID`                                                            | Passkeys are bound to a domain, by default the host of `WORK_FRONTEND_URL`. Set a parent domain to share passkeys across subdomains.                                |
| `WORK_SSO_GOOGLE_CLIENT_ID` / `_CLIENT_SECRET`                                   | Sign in with Google. Redirect URI: `<WORK_FRONTEND_URL>/api/v1/auth/sso/google/callback`                                                                            |
| `WORK_SSO_GITHUB_CLIENT_ID` / `_CLIENT_SECRET`                                   | Sign in with GitHub (`.../sso/github/callback`).                                                                                                                    |
| `WORK_SSO_MICROSOFT_CLIENT_ID` / `_CLIENT_SECRET` / `WORK_SSO_MICROSOFT_TENANT`  | Sign in with Microsoft. The tenant (id or domain) is required; `common`, `organizations` and `consumers` are refused.                                               |
| `WORK_SSO_SLACK_CLIENT_ID` / `_CLIENT_SECRET`                                    | "Sign in with Slack" (a separate app from the notification app).                                                                                                    |
| `WORK_SSO_OIDC_ISSUER`, `WORK_SSO_OIDC_CLIENT_ID`, `WORK_SSO_OIDC_CLIENT_SECRET` | Any OpenID Connect provider (Keycloak, Okta, Auth0, ...). The endpoints come from `<issuer>/.well-known/openid-configuration` at startup; the ID token is verified. |
| `WORK_SSO_OIDC_NAME`                                                             | Label of the button (default "OpenID Connect").                                                                                                                     |
| `WORK_SSO_OIDC_TRUST_EMAIL`                                                      | The provider does not send `email_verified` but you run it yourself.                                                                                                |
| `WORK_SSO_OIDC_ALLOW_HTTP`                                                       | Allow an `http://` issuer. Local testing only.                                                                                                                      |
| `WORK_SAML_IDP_METADATA_FILE`                                                    | Path to the IdP's metadata XML. The IdP must sign the assertion. ACS URL `<WORK_FRONTEND_URL>/api/v1/auth/saml/acs`, SP metadata at `.../auth/saml/metadata`.       |
| `WORK_SAML_ENTITY_ID`, `WORK_SAML_SP_KEY_FILE`                                   | SP entity id (default: the metadata URL); optional PKCS#8 PEM to sign requests.                                                                                     |

The first account created on an instance is its administrator. `work-backend make-admin <e-mail>` promotes
another. Name, description, favicon and accent colour are changed in the admin panel (`/admin`).

## E-mail

| Variable                                                                                       | Meaning                                                                                                         |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `WORK_SMTP_HOST`, `WORK_SMTP_PORT` (587), `WORK_SMTP_USER`, `WORK_SMTP_PASS`, `WORK_SMTP_FROM` | Outgoing mail: password reset, confirmation, invitations, notifications, automations. Empty host disables mail. |
| `WORK_SMTP_TLS`                                                                                | `starttls` (default), `tls` or `none` (only for a local test relay).                                            |

## Files

| Variable                                                                                                                           | Meaning                                                                                                         |
| ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `WORK_STORAGE`                                                                                                                     | `local` (default), `s3` or `azure`. The bucket or container must be private and must exist.                     |
| `WORK_UPLOAD_DIR`, `WORK_MAX_UPLOAD_BYTES`                                                                                         | Local directory (default `uploads`) and the size limit (default 5 MiB).                                         |
| `WORK_S3_BUCKET`, `WORK_S3_REGION`, `WORK_S3_ENDPOINT`, `WORK_S3_ACCESS_KEY_ID`, `WORK_S3_SECRET_ACCESS_KEY`, `WORK_S3_ALLOW_HTTP` | S3 or any S3-compatible service. Without keys the environment's identity (instance role, web identity) is used. |
| `WORK_AZURE_ACCOUNT`, `WORK_AZURE_CONTAINER`, `WORK_AZURE_ACCESS_KEY`, `WORK_AZURE_ENDPOINT`, `WORK_AZURE_ALLOW_HTTP`              | Azure Blob Storage. Without a key a managed identity is used.                                                   |

## Integrations and automations

| Variable                                  | Default | Meaning                                                                                |
| ----------------------------------------- | ------- | -------------------------------------------------------------------------------------- |
| `WORK_SLACK_CLIENT_ID` / `_CLIENT_SECRET` | -       | Slack notifications (needs `WORK_ENCRYPTION_KEY`).                                     |
| `WORK_WEBHOOK_ALLOW_PRIVATE`              | `false` | Let automations call internal addresses (SSRF protection off; local development only). |
| `WORK_WEBHOOK_ALLOW_HTTP`                 | `false` | Allow plain `http://` webhook targets.                                                 |
| `WORK_ALLOW_EXTERNAL_EMAIL`               | `false` | Let automations mail addresses that are not project members.                           |

## One-off commands

| Command                                                     | Needs                                                                 |
| ----------------------------------------------------------- | --------------------------------------------------------------------- |
| `work-backend openapi`                                      | nothing; prints the OpenAPI document                                  |
| `work-backend make-admin <e-mail>`                          | the usual configuration                                               |
| `work-backend migrate-upstream [--dry-run] [--schema NAME]` | `WORK_MIGRATE_SOURCE_URL`, see [DATA-MIGRATION.md](DATA-MIGRATION.md) |
