# Security policy

## Reporting a vulnerability

Please report vulnerabilities **privately** through GitHub: **Security → Report a vulnerability** (private
vulnerability reporting) on this repository. Do not open a public issue, pull request or discussion for them.

Include what you found, how to reproduce it, the version or commit, and what an attacker gains. We will
acknowledge a report within a few days, keep you informed while we work on a fix, and credit you in the
release notes if you wish. Please give us reasonable time to release a fix before you publish details.

## Supported versions

Security fixes go into the latest release and the `main` branch.

## Scope

In scope: the backend (`apps/backend`), the frontend (`apps/frontend`), the container images and the compose file
in `deploy/`. Out of scope: findings that need a hostile administrator (administrators are trusted), a
compromised host or database, missing hardening of a deployment you control (TLS termination, firewalling),
and denial of service by sheer volume.

## How the code protects itself

[apps/docs/src/SECURITY-MODEL.md](apps/docs/src/SECURITY-MODEL.md) lists the design rules reviewers hold changes to, the
protections feature by feature (with the tests that prove them), and the risks that remain. If you find a
behaviour that contradicts it, that is a bug worth reporting.

## Hardening a deployment

- Serve the frontend over **https** only, behind a TLS-terminating reverse proxy, and set `WORK_FRONTEND_URL`
  to that address. Passkeys and the `Secure` cookies depend on it.
- Generate every secret yourself (`openssl rand`), keep them out of images and repositories, and give the
  database account only what the application needs.
- Set `WORK_ENCRYPTION_KEY` to enable two-factor authentication, and encourage passkeys or an authenticator app.
- Keep the bucket or container for uploaded files **private**; files are served through the API only.
- Set `WORK_REGISTRATION_ENABLED=false` once your people have accounts, or configure SSO.
- Run more than one backend instance only with `WORK_RATE_LIMIT_BACKEND=postgres`.
