use super::{ApiError, json_ok};
use crate::state::AppState;
use actix_web::{web, HttpResponse};

pub async fn scan(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let repo_id = path.into_inner();
    let root = state.repo_path(&repo_id)?;
    if !std::path::Path::new(&root).is_dir() {
        return Err(ApiError::bad(format!(
            "repo path not found on disk: {root}"
        )));
    }
    let pool = state.pool_for(&repo_id)?;
    // Suffixes and info-parse rules are per repo; fall back to the global
    // defaults only when this repo has none of its own.
    let (suffixes, rules) = {
        let global = state.config.lock().unwrap().repo_defaults();
        let conn = pool.get()?;
        let (settings, _using_global) = crate::repo_settings::load(&conn, &global)?;
        (settings.include_subfixs, settings.info_parse_fns)
    };
    let root_path = std::path::PathBuf::from(&root);
    let report = web::block(move || -> anyhow::Result<crate::scanner::ScanReport> {
        let conn = pool.get()?;
        let report = crate::scanner::scan_repo(&root_path, &suffixes, &rules, &conn)?;
        crate::db::meta_set(&conn, "last_scan_at", &crate::db::now_iso())?;
        Ok(report)
    })
    .await
    .map_err(|e| ApiError::bad(format!("scan task failed: {e}")))??;
    Ok(json_ok(report))
}
