use super::{ApiError, json_ok};
use crate::config::{self, IdGenFn};
use crate::db;
use crate::repo_settings::{self, RepoSettings};
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub item_count: i64,
    pub tag_count: i64,
    pub group_count: i64,
    pub db_size: u64,
    pub last_scan: Option<String>,
    /// true when this repo has no suffix/id rules of its own and is
    /// inheriting the global defaults from config.toml.
    pub using_global: bool,
}

fn repo_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn repo_info(state: &AppState, path: &str) -> RepoInfo {
    let slug = config::repo_slug(path);
    let mut info = RepoInfo {
        id: slug.clone(),
        name: repo_name(path),
        path: path.to_string(),
        exists: std::path::Path::new(path).is_dir(),
        item_count: 0,
        tag_count: 0,
        group_count: 0,
        db_size: 0,
        last_scan: None,
        using_global: true,
    };
    let db_path = config::repo_db_path(&state.data_dir, &slug);
    info.db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    if let Ok(pool) = state.pool_for(&slug) {
        if let Ok(conn) = pool.get() {
            info.using_global =
                db::meta_get(&conn, repo_settings::KEY_INCLUDE_SUBFIXS).is_none();
            info.item_count = conn
                .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
                .unwrap_or(0);
            info.tag_count = conn
                .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
                .unwrap_or(0);
            info.group_count = conn
                .query_row("SELECT COUNT(*) FROM tag_groups", [], |r| r.get(0))
                .unwrap_or(0);
            info.last_scan = db::meta_get(&conn, "last_scan_at");
        }
    }
    info
}

pub async fn list(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    let paths = state.config.lock().unwrap().repo_paths.clone();
    let infos: Vec<RepoInfo> = paths.iter().map(|p| repo_info(&state, p)).collect();
    Ok(json_ok(infos))
}

#[derive(Deserialize)]
pub struct AddRepo {
    pub path: String,
}

pub async fn add(
    state: web::Data<AppState>,
    body: web::Json<AddRepo>,
) -> Result<HttpResponse, ApiError> {
    let raw = body.path.trim().to_string();
    if raw.is_empty() {
        return Err(ApiError::bad("path is empty"));
    }
    let canonical = std::fs::canonicalize(&raw)
        .map_err(|_| ApiError::bad(format!("path does not exist or is not accessible: {raw}")))?;
    if !canonical.is_dir() {
        return Err(ApiError::bad(format!("not a directory: {raw}")));
    }
    let stored = config::canonical_display(&canonical.to_string_lossy());
    let slug = config::repo_slug(&stored);
    {
        let cfg = state.config.lock().unwrap();
        if cfg
            .repo_paths
            .iter()
            .any(|p| config::repo_slug(p) == slug || config::canonical_display(p) == stored)
        {
            return Err(ApiError::conflict(format!("repo already added: {stored}")));
        }
    }
    // Create the database eagerly so problems surface here instead of on first use.
    let db_path = config::repo_db_path(&state.data_dir, &slug);
    let pool = db::open_pool(&db_path)?;
    // No rules are seeded here on purpose: a repo keeps inheriting the global
    // defaults from config.toml until the first time its own rules are saved,
    // so a global edit still reaches every repo that never customised.
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.repo_paths.push(stored.clone());
    }
    config::save(&state.data_dir, &state.config.lock().unwrap())?;
    state.dbs.lock().unwrap().insert(slug, pool);
    Ok(json_ok(repo_info(&state, &stored)))
}

pub async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let removed_path = {
        let mut cfg = state.config.lock().unwrap();
        let pos = cfg
            .repo_paths
            .iter()
            .position(|p| config::repo_slug(p) == id);
        match pos {
            Some(i) => Some(cfg.repo_paths.remove(i)),
            None => None,
        }
    };
    let path = match removed_path {
        Some(p) => p,
        None => return Err(ApiError::not_found(format!("repo not found: {id}"))),
    };
    config::save(&state.data_dir, &state.config.lock().unwrap())?;
    let slug = config::repo_slug(&path);
    state.dbs.lock().unwrap().remove(&slug);
    let db_path = config::repo_db_path(&state.data_dir, &slug);
    for suffix in ["", "-wal", "-shm"] {
        let p = format!("{}{}", db_path.display(), suffix);
        let _ = std::fs::remove_file(p);
    }
    Ok(json_ok(serde_json::json!({ "removed": path })))
}

/// The rules that govern one repo, plus the global defaults it can inherit.
#[derive(Serialize)]
pub struct RepoSettingsOut {
    pub include_subfixs: Vec<String>,
    pub id_gen_fns: Vec<IdGenFn>,
    pub using_global: bool,
    pub global: RepoSettings,
}

#[derive(Deserialize)]
pub struct PutRepoSettings {
    #[serde(default)]
    pub include_subfixs: Vec<String>,
    #[serde(default)]
    pub id_gen_fns: Vec<IdGenFn>,
}

pub async fn get_settings(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let repo_id = path.into_inner();
    let pool = state.pool_for(&repo_id)?;
    let conn = pool.get()?;
    let global = state.config.lock().unwrap().repo_defaults();
    let (settings, using_global) = repo_settings::load(&conn, &global)?;
    Ok(json_ok(RepoSettingsOut {
        include_subfixs: settings.include_subfixs,
        id_gen_fns: settings.id_gen_fns,
        using_global,
        global,
    }))
}

/// Save rules owned by this repo, overriding the global defaults for it.
pub async fn put_settings(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<PutRepoSettings>,
) -> Result<HttpResponse, ApiError> {
    let repo_id = path.into_inner();
    let pool = state.pool_for(&repo_id)?;
    let conn = pool.get()?;
    let mut settings = RepoSettings {
        include_subfixs: body.include_subfixs.clone(),
        id_gen_fns: body.id_gen_fns.clone(),
    };
    settings.normalize();
    repo_settings::validate(&settings)?;
    repo_settings::save(&conn, &settings)?;
    Ok(json_ok(json!({
        "include_subfixs": settings.include_subfixs,
        "id_gen_fns": settings.id_gen_fns,
        "using_global": false,
    })))
}

/// Drop this repo's own rules so it inherits the global defaults again.
pub async fn reset_settings(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let repo_id = path.into_inner();
    let pool = state.pool_for(&repo_id)?;
    let conn = pool.get()?;
    repo_settings::clear(&conn)?;
    let global = state.config.lock().unwrap().repo_defaults();
    Ok(json_ok(json!({
        "include_subfixs": global.include_subfixs,
        "id_gen_fns": global.id_gen_fns,
        "using_global": true,
    })))
}
