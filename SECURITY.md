# Security policy

Please report vulnerabilities privately through GitHub's **Security → Report a vulnerability**
(private vulnerability reporting) on this repository. Do not open a public issue for them.

Design rules the code follows, which reviewers can hold contributions to:

1. Secrets come from the environment only. No defaults, no committed credentials; startup fails if one is missing or weak.
2. Every protected handler takes an `AuthUser` and checks the caller's role on the resource's project via `authz` before touching data. Non-members receive `404`, so ids cannot be probed.
3. Request bodies are explicit structs. Never deserialize a request straight into a database row.
4. All SQL uses bound parameters. Sort and filter columns come from closed enums or allow-lists.
5. Access tokens are short-lived and live in memory. The refresh token is an `HttpOnly; SameSite=Strict` cookie, rotated on use, with reuse detection.
6. The backend is never published. It is reached only through the frontend's `/api/v1` proxy.

The audit of the upstream code that motivated these rules is in [docs/SECURITY-AUDIT.md](docs/SECURITY-AUDIT.md).
