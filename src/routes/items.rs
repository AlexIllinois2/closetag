use super::{ApiError, json_ok};
use crate::config::InfoParseFn;
use crate::db;
use crate::infoparse;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Clone)]
pub struct TagBrief {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize)]
pub struct ItemOut {
    pub id: i64,
    pub name: String,
    pub auth: String,
    pub path: String,
    pub tags: Vec<TagBrief>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct ItemsQuery {
    pub tag: Option<i64>,
    pub group: Option<i64>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// A dynamically bound SQL parameter.
#[derive(Clone)]
enum P {
    I(i64),
    S(String),
}

impl rusqlite::ToSql for P {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            P::I(v) => v.to_sql(),
            P::S(v) => v.to_sql(),
        }
    }
}

struct Filter {
    where_sql: String,
    params: Vec<P>,
}

fn build_filter(
    conn: &Connection,
    tag: Option<i64>,
    group: Option<i64>,
    q: &Option<String>,
) -> anyhow::Result<Filter> {
    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<P> = Vec::new();

    if let Some(t) = tag {
        wheres.push(
            "EXISTS (SELECT 1 FROM item_tags it WHERE it.item_id = items.id AND it.tag_id = ?)"
                .to_string(),
        );
        params.push(P::I(t));
    }

    if let Some(g) = group {
        let (includes, excludes) = super::group_rules(conn, g)?;
        if let Some(inc) = includes {
            let json = serde_json::to_string(&inc).unwrap_or_else(|_| "[]".to_string());
            wheres.push(
                "EXISTS (SELECT 1 FROM item_tags it WHERE it.item_id = items.id \
                 AND it.tag_id IN (SELECT CAST(value AS INTEGER) FROM json_each(?)))"
                    .to_string(),
            );
            params.push(P::S(json));
        }
        if !excludes.is_empty() {
            let json = serde_json::to_string(&excludes).unwrap_or_else(|_| "[]".to_string());
            wheres.push(
                "items.id NOT IN (SELECT item_id FROM item_tags \
                 WHERE tag_id IN (SELECT CAST(value AS INTEGER) FROM json_each(?)))"
                    .to_string(),
            );
            params.push(P::S(json));
        }
    }

    if let Some(q) = q {
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
        wheres.push(
            "(items.path LIKE ? ESCAPE '\\' OR items.name LIKE ? ESCAPE '\\' \
             OR items.auth LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        params.push(P::S(like.clone()));
        params.push(P::S(like.clone()));
        params.push(P::S(like));
    }

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };
    Ok(Filter { where_sql, params })
}

pub fn load_tags_for(
    conn: &Connection,
    item_ids: &[i64],
) -> rusqlite::Result<HashMap<i64, Vec<TagBrief>>> {
    let mut map: HashMap<i64, Vec<TagBrief>> = HashMap::new();
    if item_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT it.item_id, t.id, t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id \
         WHERE it.item_id IN ({placeholders}) ORDER BY t.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(item_ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            TagBrief {
                id: r.get(1)?,
                name: r.get(2)?,
            },
        ))
    })?;
    for row in rows {
        let (item_id, t) = row?;
        map.entry(item_id).or_default().push(t);
    }
    Ok(map)
}

pub fn query_items(
    conn: &Connection,
    tag: Option<i64>,
    group: Option<i64>,
    q: &Option<String>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<ItemOut>> {
    let filter = build_filter(conn, tag, group, q)?;
    let mut params = filter.params.clone();
    params.push(P::I(limit));
    params.push(P::I(offset));
    let sql = format!(
        "SELECT id, name, auth, path, created_at, updated_at FROM items {} \
         ORDER BY name, auth, path LIMIT ? OFFSET ?",
        filter.where_sql
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut raw = Vec::new();
    let mut item_ids = Vec::new();
    for row in rows {
        let (id, name, auth, path, c, u) = row?;
        raw.push((id, name, auth, path, c, u));
        item_ids.push(id);
    }
    let tags_map = load_tags_for(conn, &item_ids)?;
    Ok(raw
        .into_iter()
        .map(|(id, name, auth, path, c, u)| ItemOut {
            id,
            name,
            auth,
            path,
            tags: tags_map.get(&id).cloned().unwrap_or_default(),
            created_at: c,
            updated_at: u,
        })
        .collect())
}

pub fn count_items(
    conn: &Connection,
    tag: Option<i64>,
    group: Option<i64>,
    q: &Option<String>,
) -> anyhow::Result<i64> {
    let filter = build_filter(conn, tag, group, q)?;
    let sql = format!("SELECT COUNT(*) FROM items {}", filter.where_sql);
    Ok(conn.query_row(&sql, params_from_iter(filter.params.iter()), |r| r.get(0))?)
}

pub fn load_item(conn: &Connection, item_id: i64) -> rusqlite::Result<ItemOut> {
    let (name, auth, path, c, u) = conn.query_row(
        "SELECT name, auth, path, created_at, updated_at FROM items WHERE id = ?1",
        [item_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        },
    )?;
    let tags_map = load_tags_for(conn, &[item_id])?;
    Ok(ItemOut {
        id: item_id,
        name,
        auth,
        path,
        tags: tags_map.get(&item_id).cloned().unwrap_or_default(),
        created_at: c,
        updated_at: u,
    })
}

/// Replace the full tag set of an item inside a transaction.
pub fn set_item_tags(conn: &mut Connection, item_id: i64, tag_ids: &[i64]) -> anyhow::Result<()> {
    {
        let tx = conn.transaction()?;
        for t in tag_ids {
            let exists: Option<i64> = tx
                .query_row("SELECT id FROM tags WHERE id = ?1", [t], |r| r.get(0))
                .optional()?;
            if exists.is_none() {
                anyhow::bail!("tag not found: {t}");
            }
        }
        tx.execute("DELETE FROM item_tags WHERE item_id = ?1", [item_id])?;
        for t in tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO item_tags(item_id, tag_id) VALUES (?1, ?2)",
                params![item_id, t],
            )?;
        }
        tx.commit()?;
    }
    Ok(())
}

fn normalize_rel_path(p: &str) -> Result<String, ApiError> {
    let s = p.trim().replace('\\', "/");
    if s.is_empty() {
        return Err(ApiError::bad("path is empty"));
    }
    if s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || s == ".."
        || s.contains("/../")
    {
        return Err(ApiError::bad("path must be relative to the repo root"));
    }
    Ok(s)
}

pub async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<ItemsQuery>,
) -> Result<HttpResponse, ApiError> {
    let pool = state.pool_for(&path.into_inner())?;
    let conn = pool.get()?;
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let offset = query.offset.unwrap_or(0).max(0);
    let items = query_items(&conn, query.tag, query.group, &query.q, limit, offset)?;
    let total = count_items(&conn, query.tag, query.group, &query.q)?;
    Ok(json_ok(serde_json::json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "items": items,
    })))
}

#[derive(Deserialize)]
pub struct CreateItem {
    pub path: String,
    #[serde(default)]
    pub tags: Vec<i64>,
}

/// Info-parse rules for one repo: the repo's own settings, or the global
/// defaults when it has none of its own.
fn info_rules(state: &AppState, conn: &Connection) -> anyhow::Result<Vec<InfoParseFn>> {
    let global = state.config.lock().unwrap().repo_defaults();
    let (settings, _using_global) = crate::repo_settings::load(conn, &global)?;
    Ok(settings.info_parse_fns)
}

pub async fn create(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<CreateItem>,
) -> Result<HttpResponse, ApiError> {
    let rel = normalize_rel_path(&body.path)?;
    let pool = state.pool_for(&path.into_inner())?;
    let mut conn = pool.get()?;
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM items WHERE path = ?1", [&rel], |r| r.get(0))
        .optional()?;
    if existing.is_some() {
        return Err(ApiError::conflict(format!("item already exists: {rel}")));
    }
    let rules = info_rules(&state, &conn)?;
    let info = infoparse::parse_info(&rel, &rules);
    let now = db::now_iso();
    conn.execute(
        "INSERT INTO items(name, auth, path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![info.name, info.auth, rel, now],
    )?;
    let item_id = conn.last_insert_rowid();
    if !body.tags.is_empty() {
        set_item_tags(&mut conn, item_id, &body.tags)?;
    }
    let item = load_item(&conn, item_id)?;
    Ok(json_ok(item))
}

#[derive(Deserialize)]
pub struct UpdateItem {
    pub path: Option<String>,
    pub tags: Option<Vec<i64>>,
}

pub async fn update(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
    body: web::Json<UpdateItem>,
) -> Result<HttpResponse, ApiError> {
    let (repo, item_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let mut conn = pool.get()?;
    let current: Option<(String, String, String)> = conn
        .query_row(
            "SELECT name, auth, path FROM items WHERE id = ?1",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (cur_name, cur_auth, cur_path) = match current {
        Some(v) => v,
        None => return Err(ApiError::not_found("item not found")),
    };
    let new_path = match &body.path {
        Some(p) => normalize_rel_path(p)?,
        None => cur_path.clone(),
    };
    // name/auth 由 info_parse 规则从文件名解析；只有路径变化时才重算
    let (new_name, new_auth) = if body.path.is_some() {
        let info = infoparse::parse_info(&new_path, &info_rules(&state, &conn)?);
        (info.name, info.auth)
    } else {
        (cur_name, cur_auth)
    };
    if new_path != cur_path {
        let clash: Option<i64> = conn
            .query_row(
                "SELECT id FROM items WHERE path = ?1 AND id != ?2",
                params![new_path, item_id],
                |r| r.get(0),
            )
            .optional()?;
        if clash.is_some() {
            return Err(ApiError::conflict("another item already uses this path"));
        }
    }
    conn.execute(
        "UPDATE items SET name = ?1, auth = ?2, path = ?3, updated_at = ?4 WHERE id = ?5",
        params![new_name, new_auth, new_path, db::now_iso(), item_id],
    )?;
    if let Some(tags) = &body.tags {
        set_item_tags(&mut conn, item_id, tags)?;
    }
    let item = load_item(&conn, item_id)?;
    Ok(json_ok(item))
}

pub async fn delete(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, item_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute("DELETE FROM items WHERE id = ?1", [item_id])?;
    if n == 0 {
        return Err(ApiError::not_found("item not found"));
    }
    Ok(json_ok(serde_json::json!({ "deleted": item_id })))
}

#[derive(Deserialize)]
pub struct AddTags {
    pub tag_ids: Vec<i64>,
}

pub async fn add_tags(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
    body: web::Json<AddTags>,
) -> Result<HttpResponse, ApiError> {
    let (repo, item_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let mut conn = pool.get()?;
    let exists: Option<i64> = (&conn)
        .query_row("SELECT id FROM items WHERE id = ?1", [item_id], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(ApiError::not_found("item not found"));
    }
    let mut current: Vec<i64> = (&conn)
        .prepare("SELECT tag_id FROM item_tags WHERE item_id = ?1")?
        .query_map([item_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for t in &body.tag_ids {
        if !current.contains(t) {
            current.push(*t);
        }
    }
    set_item_tags(&mut conn, item_id, &current)?;
    let item = load_item(&conn, item_id)?;
    Ok(json_ok(item))
}

pub async fn remove_tag(
    state: web::Data<AppState>,
    path: web::Path<(String, i64, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, item_id, tag_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute(
        "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
        params![item_id, tag_id],
    )?;
    if n == 0 {
        return Err(ApiError::not_found("link not found"));
    }
    Ok(json_ok(serde_json::json!({ "deleted": { "item_id": item_id, "tag_id": tag_id } })))
}
