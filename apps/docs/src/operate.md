# Upgrade and backup

## Upgrade

1. Read the [release notes](https://github.com/EversIT/work/releases) of every version you skip.
2. [Back up](#backup-and-restore) the database.
3. Change both image tags in `.env` to the new version.
4. Apply:

   ```sh
   docker compose pull
   docker compose up -d
   ```

Database migrations run automatically when the backend starts. They only move forward, so a downgrade means
restoring a backup taken before the upgrade.

## Backup and restore

Two things hold state: the database and, unless you use S3 or Azure, the `uploads` volume. Keep `.env` too: without
`WORK_JWT_SECRET` and `WORK_ENCRYPTION_KEY`, stored two-factor secrets and Slack tokens cannot be read after a
restore.

```sh
# database
docker compose exec -T db sh -c 'pg_dump -U "$POSTGRES_USER" -Fc "$POSTGRES_DB"' > work-$(date +%F).dump

# uploads (local storage only)
docker run --rm -v work_uploads:/data -v "$PWD":/backup ubuntu:24.04 tar czf /backup/uploads-$(date +%F).tgz -C /data .

# restore the database
docker compose exec -T db sh -c 'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --clean --if-exists' < work-2026-01-31.dump
```

The volume name starts with your Compose project name (the folder name by default); check it with
`docker volume ls`. Test the restore on another machine before you need it.

## Demo

The demo is the normal frontend image started in demo mode. It needs neither a database nor a backend: every API
call is answered in the browser from made-up data, nothing is stored, and a reload starts over.

```sh
docker run -d -p 3000:3000 -e DEMO_MODE=true eversit/work-frontend:latest
```

`DEMO_MODE` is read when the server starts, so the same image serves the real app and the demo. To link the
documentation to it, set the repository variable `DOCS_DEMO_URL` to the demo's address before building the docs.
