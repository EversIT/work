//! Central authorization. Handlers never read project-scoped data without calling
//! `require_role` first. Non-members get `NotFound`, not `Forbidden`, so the existence
//! of a project cannot be probed. Members with too low a role get `Forbidden`.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "viewer" => Some(Role::Viewer),
            "member" => Some(Role::Member),
            "admin" => Some(Role::Admin),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }
}

pub async fn require_role(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    min: Role,
) -> ApiResult<Role> {
    Ok(require_membership(db, user_id, project_id, min).await?.0)
}

/// Like `require_role`, also telling whether the member has limited access: a restricted member
/// (always a viewer) sees spaces and lists only through explicit grants, never by being public.
pub async fn require_membership(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    min: Role,
) -> ApiResult<(Role, bool)> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT role, restricted FROM project_members WHERE project_id = $1 AND user_id = $2",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (role, restricted) = row
        .and_then(|(r, restricted)| Role::parse(&r).map(|r| (r, restricted)))
        .ok_or(ApiError::NotFound)?;
    if role < min {
        return Err(ApiError::Forbidden);
    }
    Ok((role, restricted))
}

/// May the caller upload files into the project? Members can; a restricted member only while they
/// hold at least a `member` grant on some list or space of the project.
pub async fn require_uploader(db: &PgPool, user_id: Uuid, project_id: Uuid) -> ApiResult<()> {
    let (role, restricted) = require_membership(db, user_id, project_id, Role::Viewer).await?;
    if role >= Role::Member {
        return Ok(());
    }
    if restricted {
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM resource_grants g WHERE g.user_id = $1 AND g.role IN ('member','admin')
               AND ((g.resource_type = 'list' AND g.resource_id IN (SELECT l.id FROM lists l JOIN workspaces w ON w.id = l.workspace_id WHERE w.project_id = $2 AND l.deleted_at IS NULL))
                 OR (g.resource_type = 'space' AND g.resource_id IN (SELECT s.id FROM spaces s JOIN workspaces w ON w.id = s.workspace_id WHERE w.project_id = $2 AND s.deleted_at IS NULL)))",
        )
        .bind(user_id)
        .bind(project_id)
        .fetch_one(db)
        .await?;
        if n.0 > 0 {
            return Ok(());
        }
    }
    Err(ApiError::Forbidden)
}

/// Resolve a workspace to its project and check the caller's role in that project.
/// Returns the project id. Unknown workspace and non-member look identical.
pub async fn require_workspace_role(
    db: &PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
    min: Role,
) -> ApiResult<Uuid> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT project_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(db)
        .await?;
    let (project_id,) = row.ok_or(ApiError::NotFound)?;
    require_role(db, user_id, project_id, min).await?;
    Ok(project_id)
}

/// Resolved location of a resource inside the tenant tree.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    pub project_id: Uuid,
    pub workspace_id: Uuid,
}

async fn grant_role(db: &PgPool, user_id: Uuid, kind: &str, id: Uuid) -> ApiResult<Option<Role>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM resource_grants WHERE resource_type = $1 AND resource_id = $2 AND user_id = $3",
    )
    .bind(kind)
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|(r,)| Role::parse(&r)))
}

/// Effective role on a space: the project role, unless the space is private (or the caller is a
/// restricted member), in which case project admins/owners keep their role and everyone else
/// needs an explicit grant.
pub async fn require_space_role(
    db: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
    min: Role,
) -> ApiResult<Scope> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT w.project_id, s.workspace_id, s.access_type FROM spaces s
         JOIN workspaces w ON w.id = s.workspace_id WHERE s.id = $1 AND s.deleted_at IS NULL",
    )
    .bind(space_id)
    .fetch_optional(db)
    .await?;
    let (project_id, workspace_id, access) = row.ok_or(ApiError::NotFound)?;
    let (project_role, restricted) =
        require_membership(db, user_id, project_id, Role::Viewer).await?;
    let effective = if (access == "public" && !restricted) || project_role >= Role::Admin {
        project_role
    } else {
        grant_role(db, user_id, "space", space_id)
            .await?
            .ok_or(ApiError::NotFound)?
    };
    if effective < min {
        return Err(ApiError::Forbidden);
    }
    Ok(Scope {
        project_id,
        workspace_id,
    })
}

/// Same for lists; a list is also hidden when its space is private.
async fn list_role(
    db: &PgPool,
    user_id: Uuid,
    (project_role, restricted): (Role, bool),
    list_id: Uuid,
) -> ApiResult<Option<(Role, Scope)>> {
    let row: Option<(Uuid, Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT w.project_id, l.workspace_id, l.space_id, l.access_type, s.access_type FROM lists l
         JOIN spaces s ON s.id = l.space_id JOIN workspaces w ON w.id = l.workspace_id
         WHERE l.id = $1 AND l.deleted_at IS NULL AND s.deleted_at IS NULL",
    )
    .bind(list_id)
    .fetch_optional(db)
    .await?;
    let Some((project_id, workspace_id, space_id, list_access, space_access)) = row else {
        return Ok(None);
    };
    let scope = Scope {
        project_id,
        workspace_id,
    };
    if (list_access == "public" && space_access == "public" && !restricted)
        || project_role >= Role::Admin
    {
        return Ok(Some((project_role, scope)));
    }
    let list_grant = grant_role(db, user_id, "list", list_id).await?;
    let space_grant = grant_role(db, user_id, "space", space_id).await?;
    Ok(list_grant.max(space_grant).map(|r| (r, scope)))
}

pub async fn require_list_role(
    db: &PgPool,
    user_id: Uuid,
    list_id: Uuid,
    min: Role,
) -> ApiResult<Scope> {
    Ok(list_access(db, user_id, list_id, min).await?.0)
}

/// `require_list_role`, also returning the caller's effective role on the list.
pub async fn list_access(
    db: &PgPool,
    user_id: Uuid,
    list_id: Uuid,
    min: Role,
) -> ApiResult<(Scope, Role)> {
    let project: Option<(Uuid,)> = sqlx::query_as(
        "SELECT w.project_id FROM lists l JOIN workspaces w ON w.id = l.workspace_id WHERE l.id = $1 AND l.deleted_at IS NULL",
    )
    .bind(list_id)
    .fetch_optional(db)
    .await?;
    let (project_id,) = project.ok_or(ApiError::NotFound)?;
    let member = require_membership(db, user_id, project_id, Role::Viewer).await?;
    let (role, scope) = list_role(db, user_id, member, list_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if role < min {
        return Err(ApiError::Forbidden);
    }
    Ok((scope, role))
}

/// A card is reachable through any list it is in. A card in no list falls back to the project
/// role, except for restricted members, who have no access to it.
pub async fn require_card_role(
    db: &PgPool,
    user_id: Uuid,
    card_id: Uuid,
    min: Role,
) -> ApiResult<Scope> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT w.project_id, c.workspace_id FROM cards c JOIN workspaces w ON w.id = c.workspace_id
         WHERE c.id = $1 AND c.deleted_at IS NULL",
    )
    .bind(card_id)
    .fetch_optional(db)
    .await?;
    let (project_id, workspace_id) = row.ok_or(ApiError::NotFound)?;
    let member = require_membership(db, user_id, project_id, Role::Viewer).await?;
    let (project_role, restricted) = member;
    let lists: Vec<(Uuid,)> = sqlx::query_as("SELECT list_id FROM card_lists WHERE card_id = $1")
        .bind(card_id)
        .fetch_all(db)
        .await?;
    let mut best = if lists.is_empty() && !restricted {
        Some(project_role)
    } else {
        None
    };
    for (list_id,) in lists {
        if let Some((r, _)) = list_role(db, user_id, member, list_id).await? {
            best = best.max(Some(r));
        }
    }
    let role = best.ok_or(ApiError::NotFound)?;
    if role < min {
        return Err(ApiError::Forbidden);
    }
    Ok(Scope {
        project_id,
        workspace_id,
    })
}

/// Generic "resolve a workspace-scoped row to its project" check used for card types, fields, etc.
pub async fn require_workspace_scoped(
    db: &PgPool,
    user_id: Uuid,
    table: &'static str,
    id: Uuid,
    min: Role,
) -> ApiResult<Scope> {
    // `table` is a compile-time constant chosen by our own code, never user input.
    let sql = format!(
        "SELECT w.project_id, t.workspace_id FROM {table} t JOIN workspaces w ON w.id = t.workspace_id WHERE t.id = $1"
    );
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(&sql).bind(id).fetch_optional(db).await?;
    let (project_id, workspace_id) = row.ok_or(ApiError::NotFound)?;
    require_role(db, user_id, project_id, min).await?;
    Ok(Scope {
        project_id,
        workspace_id,
    })
}

/// Is the user a member of the project owning the workspace? (for validating user references)
pub async fn is_project_member(db: &PgPool, project_id: Uuid, user_id: Uuid) -> ApiResult<bool> {
    let n: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM project_members WHERE project_id = $1 AND user_id = $2",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(n.0 > 0)
}

/// SQL condition (to AND into a WHERE) restricting `card_alias` to cards the user may see:
/// cards in no list, or in at least one public list of a public space, or in a list/space they
/// have a grant on. Project admins/owners see everything (`admin_ph` is a bool placeholder).
/// Restricted members (see `require_membership`) only get the grant branch: neither cards in no
/// list nor public lists count for them.
pub fn visible_cards_sql(card_alias: &str, admin_ph: usize, user_ph: usize) -> String {
    let restricted = format!(
        "EXISTS (SELECT 1 FROM project_members vpm JOIN workspaces vw ON vw.project_id = vpm.project_id
                 WHERE vw.id = {card_alias}.workspace_id AND vpm.user_id = ${user_ph} AND vpm.restricted)"
    );
    format!(
        "(${admin_ph} OR (NOT {restricted} AND NOT EXISTS (SELECT 1 FROM card_lists vx WHERE vx.card_id = {card_alias}.id)) OR EXISTS (
            SELECT 1 FROM card_lists vx JOIN lists vl ON vl.id = vx.list_id AND vl.deleted_at IS NULL
            JOIN spaces vs ON vs.id = vl.space_id AND vs.deleted_at IS NULL
            WHERE vx.card_id = {card_alias}.id AND ((vl.access_type = 'public' AND vs.access_type = 'public' AND NOT {restricted}) OR EXISTS (
                SELECT 1 FROM resource_grants vg WHERE vg.user_id = ${user_ph}
                  AND ((vg.resource_type = 'list' AND vg.resource_id = vl.id) OR (vg.resource_type = 'space' AND vg.resource_id = vs.id))))))"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_ordered() {
        assert!(
            Role::Viewer < Role::Member && Role::Member < Role::Admin && Role::Admin < Role::Owner
        );
        assert_eq!(Role::parse("owner"), Some(Role::Owner));
        assert_eq!(Role::parse("root"), None);
    }
}
