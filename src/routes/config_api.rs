use super::{ApiError, json_ok};
use crate::config::{self, Config, InfoParseFn};
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

pub async fn get(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    let cfg = state.config.lock().unwrap().clone();
    Ok(json_ok(json!({
        "config": cfg,
        "data_dir": state.data_dir.display().to_string(),
        "version": config::VERSION,
        "platform": std::env::consts::OS,
    })))
}

pub async fn put(
    state: web::Data<AppState>,
    body: web::Json<Config>,
) -> Result<HttpResponse, ApiError> {
    let mut cfg = body.into_inner();
    cfg.include_subfixs = cfg
        .include_subfixs
        .iter()
        .map(|s| config::normalize_suffix(s))
        .filter(|s| !s.is_empty())
        .collect();
    let mut seen = std::collections::HashSet::new();
    cfg.repo_paths
        .retain(|p| {
            let c = config::canonical_display(p);
            !c.is_empty() && seen.insert(c)
        });
    cfg.validate()?;
    config::save(&state.data_dir, &cfg)?;
    *state.config.lock().unwrap() = cfg;
    state.prune_dbs();
    Ok(json_ok(json!({ "ok": true })))
}

pub async fn about(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    Ok(json_ok(json!({
        "version": config::VERSION,
        "data_dir": state.data_dir.display().to_string(),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "port": state.port.load(std::sync::atomic::Ordering::Relaxed),
    })))
}

#[derive(Deserialize)]
pub struct InfoParsePreviewReq {
    pub path: String,
    /// Rules to evaluate; when omitted the rules of `repo` (or the global
    /// defaults) are used.
    pub rules: Option<Vec<InfoParseFn>>,
    pub repo: Option<String>,
}

pub async fn infoparse_preview(
    state: web::Data<AppState>,
    body: web::Json<InfoParsePreviewReq>,
) -> Result<HttpResponse, ApiError> {
    let rules = match &body.rules {
        Some(r) if !r.is_empty() => r.clone(),
        _ => match &body.repo {
            Some(repo) => {
                let pool = state.pool_for(repo)?;
                let conn = pool.get()?;
                let global = state.config.lock().unwrap().repo_defaults();
                let (settings, _using_global) = crate::repo_settings::load(&conn, &global)?;
                settings.info_parse_fns
            }
            None => state.config.lock().unwrap().info_parse_fns.clone(),
        },
    };
    let rel = body.path.trim().replace('\\', "/");
    if rel.is_empty() {
        return Err(ApiError::bad("path is empty"));
    }
    let result = crate::infoparse::parse_info(&rel, &rules);
    let mut per_rule = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        let (matched, name, auth) = match crate::infoparse::eval_rule(&rel, rule) {
            Some((n, a)) => (true, Some(n), Some(a)),
            None => {
                let matched = match &rule.match_re {
                    Some(re) => regex::Regex::new(re).map(|r| r.is_match(&rel)).unwrap_or(false),
                    None => true,
                };
                (matched, None, None)
            }
        };
        per_rule.push(json!({ "index": i, "matched": matched, "name": name, "auth": auth }));
    }
    Ok(json_ok(json!({
        "name": result.name,
        "auth": result.auth,
        "rule_index": result.rule_index,
        "used_fallback": result.used_fallback,
        "rules": per_rule,
    })))
}
