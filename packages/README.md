# packages

Shared, non-app code of the workspace. Anything that is not something you run on its own belongs here.

| Folder    | Contents                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------ |
| `docker/` | Dockerfiles for every image (`next`, `rust`, `static`) and the nginx config of the static image. |

Libraries shared between apps go in here as their own folders (for example `packages/ui` or `packages/shared`),
each with a `project.json`, so Nx knows about them and `nx affected` rebuilds what depends on a change.
The build context of every image is the repository root; the `container` targets pass the Dockerfile as
`{workspaceRoot}/packages/docker/<name>.Dockerfile`.
