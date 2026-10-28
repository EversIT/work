<!-- The title must read `type(scope): subject`, for example `fix(backend): reject expired invites`. -->

## What and why

<!-- What does this change, and why? Link the issue: "Closes #123". -->

## How it was tested

<!-- Tests you added or ran, and anything you checked by hand. -->

## Checklist

- [ ] The change is focused (one concern) and the code reads clearly
- [ ] Tests cover it, including who must be **refused** for access-related changes
- [ ] `cargo fmt`, `backend:lint`, `backend:test` (with `--include-ignored` and a database), `frontend:lint`, `frontend:test` pass
- [ ] API changed? `apps/backend/openapi.json` is regenerated and committed
- [ ] New or changed setting? `apps/docs/src/CONFIGURATION.md`, `packages/deploy/.env.example` and `packages/deploy/docker-compose.yml` are updated; no secret has a default
- [ ] New text? It is in both `messages/en.json` and `messages/de.json`; no hard-coded colours (design tokens only)
- [ ] Docs updated where behaviour changed
- [ ] No secrets, tokens or personal data in the diff

## Notes for reviewers

<!-- Trade-offs, things left out on purpose, follow-ups. -->
