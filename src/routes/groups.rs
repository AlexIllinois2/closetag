use super::{ApiError, json_ok};
use crate::db;
use crate::routes::items;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct GroupOut {
    pub id: i64,
    pub name: String,
    /// `null` means the group includes every item.
    pub includes: Option<Vec<i64>>,
    pub excludes: Vec<i64>,
    pub description: String,
    pub item_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

fn parse_opt_ids(s: Option<String>) -> Option<Vec<i64>> {
    s.and_then(|s| serde_json::from_str::<Vec<i64>>(&s).ok())
}

fn parse_ids(s: String) -> Vec<i64> {
    serde_json::from_str(&s).unwrap_or_default()
}

fn validate_tags(conn: &Connection, ids: &[i64]) -> anyhow::Result<()> {
    for id in ids {
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM tags WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        if exists.is_none() {
            anyhow::bail!("tag not found: {id}");
        }
    }
    Ok(())
}

pub async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let pool = state.pool_for(&path.into_inner())?;
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, name, includes, excludes, description, created_at, updated_at \
         FROM tag_groups ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, inc, exc, desc, c, u) = row?;
        let item_count = items::count_items(&conn, None, Some(id), &None).unwrap_or(0);
        out.push(GroupOut {
            id,
            name,
            includes: parse_opt_ids(inc),
            excludes: parse_ids(exc),
            description: desc,
            item_count,
            created_at: c,
            updated_at: u,
        });
    }
    Ok(json_ok(out))
}

#[derive(Deserialize)]
pub struct CreateGroup {
    pub name: String,
    /// `null` or absent = include all items.
    pub includes: Option<Vec<i64>>,
    #[serde(default)]
    pub excludes: Vec<i64>,
    #[serde(default)]
    pub description: String,
}

pub async fn create(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<CreateGroup>,
) -> Result<HttpResponse, ApiError> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad("group name is empty"));
    }
    let pool = state.pool_for(&path.into_inner())?;
    let conn = pool.get()?;
    validate_tags(&conn, body.includes.as_deref().unwrap_or(&[]))?;
    validate_tags(&conn, &body.excludes)?;
    let includes_json = body
        .includes
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
    let excludes_json = serde_json::to_string(&body.excludes).unwrap_or_else(|_| "[]".to_string());
    let now = db::now_iso();
    conn.execute(
        "INSERT INTO tag_groups(name, includes, excludes, description, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![name, includes_json, excludes_json, body.description, now],
    )
    .map_err(|e| {
        if e.to_string().to_lowercase().contains("unique") {
            ApiError::conflict(format!("group already exists: {name}"))
        } else {
            ApiError::from(anyhow::Error::new(e))
        }
    })?;
    let id = conn.last_insert_rowid();
    let group = load_group(&conn, id)?;
    Ok(json_ok(group))
}

fn load_group(conn: &Connection, id: i64) -> anyhow::Result<GroupOut> {
    let row: Option<(i64, String, Option<String>, String, String, String, String)> = conn
        .query_row(
            "SELECT id, name, includes, excludes, description, created_at, updated_at \
             FROM tag_groups WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .optional()?;
    let (id, name, inc, exc, desc, c, u) = match row {
        Some(v) => v,
        None => anyhow::bail!("group not found: {id}"),
    };
    let item_count = items::count_items(conn, None, Some(id), &None).unwrap_or(0);
    Ok(GroupOut {
        id,
        name,
        includes: parse_opt_ids(inc),
        excludes: parse_ids(exc),
        description: desc,
        item_count,
        created_at: c,
        updated_at: u,
    })
}

#[derive(Deserialize)]
pub struct UpdateGroup {
    pub name: Option<String>,
    /// `None` = leave unchanged, `Some(None)` = include all items.
    #[serde(default)]
    pub includes: Option<Option<Vec<i64>>>,
    pub excludes: Option<Vec<i64>>,
    pub description: Option<String>,
}

pub async fn update(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
    body: web::Json<UpdateGroup>,
) -> Result<HttpResponse, ApiError> {
    let (repo, group_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let current = load_group(&conn, group_id).map_err(|_| ApiError::not_found("group not found"))?;
    let name = body
        .name
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(current.name);
    let includes = match &body.includes {
        Some(v) => v.clone(),
        None => current.includes,
    };
    let excludes = body.excludes.clone().unwrap_or(current.excludes);
    let description = body.description.clone().unwrap_or(current.description);
    validate_tags(&conn, includes.as_deref().unwrap_or(&[]))?;
    validate_tags(&conn, &excludes)?;
    let includes_json = includes
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
    let excludes_json = serde_json::to_string(&excludes).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE tag_groups SET name = ?1, includes = ?2, excludes = ?3, description = ?4, \
         updated_at = ?5 WHERE id = ?6",
        params![name, includes_json, excludes_json, description, db::now_iso(), group_id],
    )
    .map_err(|e| {
        if e.to_string().to_lowercase().contains("unique") {
            ApiError::conflict(format!("group already exists: {name}"))
        } else {
            ApiError::from(anyhow::Error::new(e))
        }
    })?;
    let group = load_group(&conn, group_id)?;
    Ok(json_ok(group))
}

pub async fn delete(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, group_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute("DELETE FROM tag_groups WHERE id = ?1", [group_id])?;
    if n == 0 {
        return Err(ApiError::not_found("group not found"));
    }
    Ok(json_ok(serde_json::json!({ "deleted": group_id })))
}

pub async fn items(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
    query: web::Query<items::ItemsQuery>,
) -> Result<HttpResponse, ApiError> {
    let (repo, group_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let offset = query.offset.unwrap_or(0).max(0);
    let page = items::query_items(&conn, None, Some(group_id), &query.q, limit, offset)?;
    let total = items::count_items(&conn, None, Some(group_id), &query.q)?;
    Ok(json_ok(serde_json::json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "items": page,
    })))
}
