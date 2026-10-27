# Using the API

Everything the UI does is a documented REST call. The OpenAPI document of your instance is at
`/api/v1/openapi.json`.

```sh
curl -H "Authorization: Bearer wrk_…" https://work.example.com/api/v1/me
curl -u you@example.com:wrk_… https://work.example.com/api/v1/projects
curl -X POST -H "Authorization: Bearer wrk_…" -H "content-type: application/json" \
     -d '{"data":{"title":"Created through the API"}}' https://work.example.com/api/v1/lists/<list-id>/cards
```

Create tokens under **Settings → API tokens**. A token is shown once, is read-only or read-write, can expire, acts
with your permissions and can be revoked at any time. Managing tokens, passwords, sessions and the account itself
needs a browser session, so a leaked token cannot mint more tokens or sign you out elsewhere.
