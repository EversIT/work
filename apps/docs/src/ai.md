# AI assistant and MCP

Two things live here: an **assistant** inside the app (the sparkle menus and buttons), and an **MCP server** so AI
agents can work with your issues. Both run on your own infrastructure. There is no cloud AI service, no API key
and nothing leaves your servers.

The two are independent: the assistant is **off until you set a self-hosted language model**; the MCP server is
always on and needs no model.

## Turning the assistant on

The AI features appear in the app exactly while a language model is set, and are hidden everywhere while none is.

1. Run a model on your own hardware, on any server that speaks the OpenAI chat protocol
   (`/v1/chat/completions`): [llama.cpp](https://github.com/ggml-org/llama.cpp), [Ollama](https://ollama.com),
   vLLM or LocalAI. The bundled compose profile starts llama.cpp (see below).
2. Sign in as an instance administrator and open **Administration → AI**. Enter the server address (for example
   `http://model:8081` or `http://localhost:11434`) and, if the server needs one, the model name (Ollama does).
3. Press **Test connection**: the backend asks the model for one word and tells you in a few fixed words what went
   wrong, if anything. Then **Save**. The features are on for everybody at once.

**Remove setting** turns them off again, unless the server environment names a model (see below).

The address must be plain `http://` or `https://` without a login, query or fragment. There is no API key field on
purpose: the model belongs on your own network, and anything that needs a secret belongs in front of it. The
backend sends issue text only to this address. Every change is written to the administration's audit log
(`ai.set`, `ai.remove`).

**From the environment.** Instead of the app you can set `WORK_AI_MODEL_URL` (and optionally
`WORK_AI_MODEL_NAME`, `WORK_AI_MAX_OUTPUT_TOKENS`, see [Configuration](/CONFIGURATION)). That is used while the
administration has no setting of its own; a setting saved in the app wins. With several backend instances, a change
reaches all of them within about 15 seconds.

### The bundled llama.cpp service

It runs next to the backend on an internal network with no internet access.

```sh
# 1. put a GGUF model of your choice into ./models, then
echo 'WORK_AI_MODEL_URL=http://model:8081' >> .env
docker compose -f packages/deploy/docker-compose.yml --env-file .env --profile ai up -d
```

The model needs memory for its weights (a 1.5B model at 4 bit is about 1.2 GB) and is faster with a GPU, but a
small model is usable on a CPU. Requests are limited to 20 per user and minute. Everything the assistant returns is a
suggestion: nothing changes until a person presses _Apply_, and applying is an ordinary edit with their permissions.

## What it does

| Where                       | What it does                                                                                                                                                                  |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Issue, sparkle menu         | Summarize; find related issues and likely duplicates; suggest field values; suggest child issues; improve, fix spelling, shorten, expand, professional, friendly; translate.  |
| Create issue                | Similar issues while you type the summary; "Suggest" fills empty choice fields; "Write description for me" drafts a description by issue type.                                |
| Description                 | A shortcut under it: "Write description for me" while it is empty, "Improve" once it is not.                                                                                  |
| Editor bar (every text box) | "Write with AI": improve, fix, shorten, expand, friendlier, more professional, translate the selection or the whole text; shown beside the original, replaced only on Accept. |
| Comment box                 | Quick replies for the latest comment; "Check tone" offers a friendlier version.                                                                                               |
| Comments                    | "Summarize discussion" for threads of more than 8 comments.                                                                                                                   |
| Details, link an issue      | Suggested values for empty fields with one-click accept; similar issues first when linking, a close match preselects "duplicates".                                            |
| Filter (list views)         | "Describe the filter": a sentence such as "bugs assigned to me, high priority, created last week" (or in German) becomes conditions you can change.                           |
| Search box                  | "Ask": the same sentence runs as conditions in every project; what was understood is shown above the results.                                                                 |
| Board, sprint, backlog      | "Summarize" the board; "Suggest sprint", "Estimate" and "Group by topic" for the backlog; "Generate sprint notes" when completing a sprint.                                   |
| Home page, notifications    | "For you" and "Catch me up": what happened since your last visit, grouped by issue.                                                                                           |
| Project settings            | "Suggest statuses" and "Suggest fields" from a short description (templates for software, bugs, support, sales, hiring, marketing).                                           |
| Automations                 | "Describe your rule" drafts a rule from a sentence, "Explain this rule" says it in one sentence, failed runs get a reason and a summary.                                      |

Translation goes into English, German, French, Spanish, Italian, Portuguese, Dutch, Polish, Turkish, Japanese or
Chinese; how well depends on the model you run.

Parts of this do not ask the model for the answer. They are computed in the backend or in the browser from your
issues, and the model, where it takes part, only words the result:

- **Related issues, duplicates, field suggestions, estimates and sprint plans** are classic text statistics (TF-IDF with
  a light stemmer for English and German). They "learn" from your own issues: the index is built from the issues the
  caller may see, on every request, so there is nothing to train and nobody gets a hint about an issue they cannot open.
- **The sentence "rules"** (filters, search, automations, statuses and fields) are patterns, not a model: they
  understand the usual ways to say a condition or a trigger and tell you what they did not understand.

They still belong to the AI features and switch on and off with the model, so the app looks the same everywhere.

## MCP server

The app explains it too: **Account settings → API tokens** shows the address, the command to connect Claude Code,
a JSON configuration for other clients and the list of tools, below the tokens the agent signs in with. It works
whether or not the AI features are on.

The endpoint is `POST <WORK_FRONTEND_URL>/api/v1/mcp` (Streamable HTTP, JSON responses, no sessions). It
authenticates with a [personal API token](/api): `Authorization: Bearer wrk_…`. A tool does exactly what the
matching API route does with your permissions, so an agent can never reach more than you can.

**Claude Code**

```sh
claude mcp add --transport http work https://work.example.com/api/v1/mcp \
  --header "Authorization: Bearer $WORK_TOKEN"
```

**Cursor and other clients** (`mcp.json`)

```json
{
  "mcpServers": {
    "work": {
      "url": "https://work.example.com/api/v1/mcp",
      "headers": { "Authorization": "Bearer ${env:WORK_TOKEN}" }
    }
  }
}
```

Keep the token in an environment variable or your client's secret store, not in a file you commit.

| Tool                  | Changes data | What it does                                                       |
| --------------------- | ------------ | ------------------------------------------------------------------ |
| `list_projects`       | no           | Your projects, their workspaces and issue key prefixes.            |
| `list_lists`          | no           | Spaces and lists of a workspace, with each list's statuses.        |
| `search_issues`       | no           | Issues whose title or key contains a text, newest change first.    |
| `get_issue`           | no           | Description (Markdown), fields, status, child issues and comments. |
| `find_related_issues` | no           | Likely duplicates and related issues.                              |
| `create_issue`        | yes          | New issue in a list, optionally as a child of another issue.       |
| `update_issue`        | yes          | Title, description (Markdown) and other fields.                    |
| `transition_issue`    | yes          | Move an issue to another status by name.                           |
| `add_comment`         | yes          | Comment (Markdown).                                                |

There is deliberately no delete tool.

### Using it safely

- **Use a read-only token** for agents that only need to look. A `read` token can call the read tools and nothing
  else. Create separate tokens per agent so you can revoke one.
- **Issue text is untrusted input to the agent.** Anyone who can write an issue can write instructions into it. That
  is true for every tool that hands text to a model; the server marks the text as data but cannot prevent an agent
  from following it. Give agents narrowly permissioned accounts, prefer read-only tokens, and review what a
  write-capable agent did (everything shows in the issue history under the token's owner).
- Requests from a browser page of another origin are refused, and the usual rate limit per token applies.
- The assistant's model has no tools and writes nothing; its only output is text a person reviews.
