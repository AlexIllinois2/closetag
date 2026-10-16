use super::{ApiError, json_ok};
use crate::db;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct TagOut {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub description: String,
    pub item_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

const SELECT_TAG: &str = "SELECT t.id, t.name, t.parent_id, t.description, \
    (SELECT COUNT(*) FROM item_tags it WHERE it.tag_id = t.id), t.created_at, t.updated_at \
    FROM tags t";

fn row_to_tag(r: &rusqlite::Row) -> rusqlite::Result<TagOut> {
    Ok(TagOut {
        id: r.get(0)?,
        name: r.get(1)?,
        parent_id: r.get(2)?,
        description: r.get(3)?,
        item_count: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

fn tag_exists(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    Ok(found.is_some())
}

pub async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let pool = state.pool_for(&path.into_inner())?;
    let conn = pool.get()?;
    let mut stmt = conn.prepare(&format!(
        "{SELECT_TAG} ORDER BY COALESCE(t.parent_id, 0), t.name"
    ))?;
    let tags = stmt
        .query_map([], row_to_tag)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json_ok(tags))
}

#[derive(Deserialize)]
pub struct CreateTag {
    pub name: String,
    pub parent_id: Option<i64>,
    #[serde(default)]
    pub description: String,
}

pub async fn create(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<CreateTag>,
) -> Result<HttpResponse, ApiError> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad("tag name is empty"));
    }
    let pool = state.pool_for(&path.into_inner())?;
    let conn = pool.get()?;
    if let Some(pid) = body.parent_id {
        if !tag_exists(&conn, pid)? {
            return Err(ApiError::bad("parent tag not found"));
        }
    }
    let now = db::now_iso();
    conn.execute(
        "INSERT INTO tags(name, parent_id, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![name, body.parent_id, body.description, now],
    )?;
    let id = conn.last_insert_rowid();
    let tag = conn.query_row(&format!("{SELECT_TAG} WHERE t.id = ?1"), [id], row_to_tag)?;
    Ok(json_ok(tag))
}

#[derive(Deserialize)]
pub struct UpdateTag {
    pub name: Option<String>,
    /// `None` = leave unchanged, `Some(None)` = move to root, `Some(Some(id))` = set parent.
    #[serde(default)]
    pub parent_id: Option<Option<i64>>,
    pub description: Option<String>,
}

pub async fn update(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
    body: web::Json<UpdateTag>,
) -> Result<HttpResponse, ApiError> {
    let (repo, tag_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let current: Option<(String, Option<i64>, String)> = conn
        .query_row(
            "SELECT name, parent_id, description FROM tags WHERE id = ?1",
            [tag_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (cur_name, cur_parent, cur_desc) = match current {
        Some(v) => v,
        None => return Err(ApiError::not_found("tag not found")),
    };
    let name = body
        .name
        .clone()
        .map(|s| s.trim().to_string())
        .unwrap_or(cur_name);
    if name.is_empty() {
        return Err(ApiError::bad("tag name is empty"));
    }
    let parent = match &body.parent_id {
        Some(v) => *v,
        None => cur_parent,
    };
    if parent == Some(tag_id) {
        return Err(ApiError::bad("a tag cannot be its own parent"));
    }
    if let Some(pid) = parent {
        if !tag_exists(&conn, pid)? {
            return Err(ApiError::bad("parent tag not found"));
        }
        // cycle check: walk up the ancestor chain
        let mut cursor = Some(pid);
        let mut hops = 0;
        while let Some(c) = cursor {
            if c == tag_id {
                return Err(ApiError::bad("cycle detected in tag tree"));
            }
            hops += 1;
            if hops > 10_000 {
                break;
            }
            cursor = conn
                .query_row(
                    "SELECT parent_id FROM tags WHERE id = ?1",
                    [c],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten();
        }
    }
    let desc = body.description.clone().unwrap_or(cur_desc);
    conn.execute(
        "UPDATE tags SET name = ?1, parent_id = ?2, description = ?3, updated_at = ?4 WHERE id = ?5",
        params![name, parent, desc, db::now_iso(), tag_id],
    )?;
    let tag = conn.query_row(&format!("{SELECT_TAG} WHERE t.id = ?1"), [tag_id], row_to_tag)?;
    Ok(json_ok(tag))
}

pub async fn delete(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, tag_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
    if n == 0 {
        return Err(ApiError::not_found("tag not found"));
    }
    // Children are reparented to root by the ON DELETE SET NULL foreign key,
    // and item links are removed by ON DELETE CASCADE.
    Ok(json_ok(serde_json::json!({ "deleted": tag_id })))
}
