use super::{ApiError, json_ok};
use crate::config::IdGenFn;
use crate::db;
use crate::idgen;
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
    pub pk: i64,
    pub id: String,
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
            "EXISTS (SELECT 1 FROM item_tags it WHERE it.item_pk = items.pk AND it.tag_id = ?)"
                .to_string(),
        );
        params.push(P::I(t));
    }

    if let Some(g) = group {
        let (includes, excludes) = super::group_rules(conn, g)?;
        if let Some(inc) = includes {
            let json = serde_json::to_string(&inc).unwrap_or_else(|_| "[]".to_string());
            wheres.push(
                "EXISTS (SELECT 1 FROM item_tags it WHERE it.item_pk = items.pk \
                 AND it.tag_id IN (SELECT CAST(value AS INTEGER) FROM json_each(?)))"
                    .to_string(),
            );
            params.push(P::S(json));
        }
        if !excludes.is_empty() {
            let json = serde_json::to_string(&excludes).unwrap_or_else(|_| "[]".to_string());
            wheres.push(
                "items.pk NOT IN (SELECT item_pk FROM item_tags \
                 WHERE tag_id IN (SELECT CAST(value AS INTEGER) FROM json_each(?)))"
                    .to_string(),
            );
            params.push(P::S(json));
        }
    }

    if let Some(q) = q {
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
        wheres.push("(items.path LIKE ? ESCAPE '\\' OR items.id LIKE ? ESCAPE '\\')".to_string());
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
    pks: &[i64],
) -> rusqlite::Result<HashMap<i64, Vec<TagBrief>>> {
    let mut map: HashMap<i64, Vec<TagBrief>> = HashMap::new();
    if pks.is_empty() {
        return Ok(map);
    }
    let placeholders = pks.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT it.item_pk, t.id, t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id \
         WHERE it.item_pk IN ({placeholders}) ORDER BY t.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(pks.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            TagBrief {
                id: r.get(1)?,
                name: r.get(2)?,
            },
        ))
    })?;
    for row in rows {
        let (pk, t) = row?;
        map.entry(pk).or_default().push(t);
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
        "SELECT pk, id, path, created_at, updated_at FROM items {} \
         ORDER BY id, path LIMIT ? OFFSET ?",
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
        ))
    })?;
    let mut raw = Vec::new();
    let mut pks = Vec::new();
    for row in rows {
        let (pk, id, path, c, u) = row?;
        raw.push((pk, id, path, c, u));
        pks.push(pk);
    }
    let tags_map = load_tags_for(conn, &pks)?;
    Ok(raw
        .into_iter()
        .map(|(pk, id, path, c, u)| ItemOut {
            pk,
            id,
            path,
            tags: tags_map.get(&pk).cloned().unwrap_or_default(),
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

pub fn load_item(conn: &Connection, pk: i64) -> rusqlite::Result<ItemOut> {
    let (id, path, c, u) = conn.query_row(
        "SELECT id, path, created_at, updated_at FROM items WHERE pk = ?1",
        [pk],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        },
    )?;
    let tags_map = load_tags_for(conn, &[pk])?;
    Ok(ItemOut {
        pk,
        id,
        path,
        tags: tags_map.get(&pk).cloned().unwrap_or_default(),
        created_at: c,
        updated_at: u,
    })
}

/// Replace the full tag set of an item inside a transaction.
pub fn set_item_tags(conn: &mut Connection, pk: i64, tag_ids: &[i64]) -> anyhow::Result<()> {
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
        tx.execute("DELETE FROM item_tags WHERE item_pk = ?1", [pk])?;
        for t in tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO item_tags(item_pk, tag_id) VALUES (?1, ?2)",
                params![pk, t],
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

/// Id rules for one repo: the repo's own settings, or the global defaults
/// when it has none of its own.
fn id_rules(state: &AppState, conn: &Connection) -> anyhow::Result<Vec<IdGenFn>> {
    let global = state.config.lock().unwrap().repo_defaults();
    let (settings, _using_global) = crate::repo_settings::load(conn, &global)?;
    Ok(settings.id_gen_fns)
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
        .query_row("SELECT pk FROM items WHERE path = ?1", [&rel], |r| r.get(0))
        .optional()?;
    if existing.is_some() {
        return Err(ApiError::conflict(format!("item already exists: {rel}")));
    }
    let rules = id_rules(&state, &conn)?;
    let id = idgen::generate_id(&rel, &rules).id;
    let now = db::now_iso();
    conn.execute(
        "INSERT INTO items(id, path, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, rel, now],
    )?;
    let pk = conn.last_insert_rowid();
    if !body.tags.is_empty() {
        set_item_tags(&mut conn, pk, &body.tags)?;
    }
    let item = load_item(&conn, pk)?;
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
    let (repo, pk) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let mut conn = pool.get()?;
    let current: Option<(String, String)> = conn
        .query_row(
            "SELECT id, path FROM items WHERE pk = ?1",
            [pk],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (cur_id, cur_path) = match current {
        Some(v) => v,
        None => return Err(ApiError::not_found("item not found")),
    };
    let new_path = match &body.path {
        Some(p) => normalize_rel_path(p)?,
        None => cur_path.clone(),
    };
    let new_id = if body.path.is_some() {
        let rules = id_rules(&state, &conn)?;
        idgen::generate_id(&new_path, &rules).id
    } else {
        cur_id
    };
    if new_path != cur_path {
        let clash: Option<i64> = conn
            .query_row(
                "SELECT pk FROM items WHERE path = ?1 AND pk != ?2",
                params![new_path, pk],
                |r| r.get(0),
            )
            .optional()?;
        if clash.is_some() {
            return Err(ApiError::conflict("another item already uses this path"));
        }
    }
    conn.execute(
        "UPDATE items SET id = ?1, path = ?2, updated_at = ?3 WHERE pk = ?4",
        params![new_id, new_path, db::now_iso(), pk],
    )?;
    if let Some(tags) = &body.tags {
        set_item_tags(&mut conn, pk, tags)?;
    }
    let item = load_item(&conn, pk)?;
    Ok(json_ok(item))
}

pub async fn delete(
    state: web::Data<AppState>,
    path: web::Path<(String, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, pk) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute("DELETE FROM items WHERE pk = ?1", [pk])?;
    if n == 0 {
        return Err(ApiError::not_found("item not found"));
    }
    Ok(json_ok(serde_json::json!({ "deleted": pk })))
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
    let (repo, pk) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let mut conn = pool.get()?;
    let exists: Option<i64> = (&conn)
        .query_row("SELECT pk FROM items WHERE pk = ?1", [pk], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(ApiError::not_found("item not found"));
    }
    let mut current: Vec<i64> = (&conn)
        .prepare("SELECT tag_id FROM item_tags WHERE item_pk = ?1")?
        .query_map([pk], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for t in &body.tag_ids {
        if !current.contains(t) {
            current.push(*t);
        }
    }
    set_item_tags(&mut conn, pk, &current)?;
    let item = load_item(&conn, pk)?;
    Ok(json_ok(item))
}

pub async fn remove_tag(
    state: web::Data<AppState>,
    path: web::Path<(String, i64, i64)>,
) -> Result<HttpResponse, ApiError> {
    let (repo, pk, tag_id) = path.into_inner();
    let pool = state.pool_for(&repo)?;
    let conn = pool.get()?;
    let n = conn.execute(
        "DELETE FROM item_tags WHERE item_pk = ?1 AND tag_id = ?2",
        params![pk, tag_id],
    )?;
    if n == 0 {
        return Err(ApiError::not_found("link not found"));
    }
    Ok(json_ok(serde_json::json!({ "deleted": { "item_pk": pk, "tag_id": tag_id } })))
}
