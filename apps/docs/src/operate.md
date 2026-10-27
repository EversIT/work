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

## Documentation and demo on one address

The documentation and the demo are separate images that need neither a database nor a backend. The docs image can
show the demo under its own address, so you do not need a subdomain, a second certificate or a second proxy entry:

```sh
curl -fsSLO https://raw.githubusercontent.com/EversIT/work/main/deploy/docker-compose.docs.yml
docker compose -f docker-compose.docs.yml up -d
```

- `http://127.0.0.1:8080/` is the documentation, `http://127.0.0.1:8080/demo` the demo (made-up data, nothing is
  stored, a reload starts over). The navigation of the documentation links to it.
- Point your TLS-terminating reverse proxy at the **docs** container only (`reverse_proxy 127.0.0.1:8080`). The docs
  container passes `/demo` on to the demo container over the internal network.
- The demo is built to live below `/demo`. If you run it on its own, open `http://localhost:3000/demo`.
- `DEMO_UPSTREAM` (default `demo:3000`) tells the docs container where the demo is. Without a demo the documentation
  works as before and `/demo` answers with an error.

```sh
docker run -d -p 8080:8080 eversit/work-docs:latest    # only the documentation
docker run -d -p 3000:3000 eversit/work-demo:latest    # only the demo, at /demo
```
