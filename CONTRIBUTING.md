# Contributing

Thank you for helping. This project aims to be a tool people can run for years, so it values small,
reviewable changes, tests that prove behaviour, and honesty about what is not done.

By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Please report security problems
privately as described in [SECURITY.md](SECURITY.md), not as a public issue.

## Ways to contribute

- **Report a bug** or **suggest a feature** with the issue forms. Search first; add what you know (version,
  how to reproduce, what you expected).
- **Improve the docs.** A confusing sentence is a bug.
- **Translate.** Add a language by copying `apps/frontend/messages/en.json` (keep every key).
- **Send code.** For anything larger than a fix, open an issue first so we can agree on the direction before
  you spend a weekend on it.

## Your first pull request

1. Fork, then branch from `main`.
2. Set up as described in [apps/docs/src/DEVELOPMENT.md](apps/docs/src/DEVELOPMENT.md); read [apps/docs/src/ARCHITECTURE.md](apps/docs/src/ARCHITECTURE.md)
   for where things live.
3. Make the change **with tests**:
   - a backend change: an end-to-end test in `apps/backend/src/tests/` that also covers who must be _refused_;
   - a frontend helper: a unit test next to it (`*.test.ts`);
   - something users see: extend or add a Playwright test in `apps/frontend-e2e/`.
4. Run what CI runs:
   ```sh
   cargo fmt --all -- --check
   npx nx run backend:lint
   DATABASE_URL=postgres://… npx nx run backend:test --args=--include-ignored
   npx nx run frontend:lint
   npx nx run frontend:test
   npx nx run frontend:build
   ```
   After an API change also run `npx nx run backend:openapi` and commit `apps/backend/openapi.json`.
5. Open the pull request. The **title** must follow `type(scope): subject` (for example
   `fix(backend): reject expired invites`); see `scripts/commit-lint.js` for the types and scopes. Fill in the
   template, and say how you tested it.

## What reviewers look for

- **Security first.** Secrets only from the environment; every handler checks the caller's role through
  `security/authz.rs`; bodies are explicit structs; SQL is parameterised; untrusted text is never rendered as
  HTML. See [apps/docs/src/SECURITY-MODEL.md](apps/docs/src/SECURITY-MODEL.md).
- **Small and focused.** One concern per pull request. Do not mix a refactor with a feature.
- **Readable code.** Names that say what things are, comments that say _why_, no dead code, no clever tricks.
  Match the surrounding style (`cargo fmt`, Prettier on commit).
- **Honest scope.** If something is incomplete or unverified, say so in the code or the description.
- **No hard-coded colours or strings** in the frontend: use design tokens and `next-intl` (both languages).
- **Dependencies are a cost.** Add one only when it pays for itself, and prefer maintained, pure-Rust or
  well-known packages.

## Commit messages

`type(scope): subject`, lower case, imperative, at most 100 characters. Examples:

```
feat(backend): add date-based automation triggers
fix(frontend): keep the editor focused after a toolbar click
docs(docs): describe the export contract
```

## Licensing

By contributing you agree that your contribution is licensed under the project's [MIT License](LICENSE).
