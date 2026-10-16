//! Per-repo rules: the file-suffix filter and the id-generation rules.
//!
//! closetag keeps one SQLite database per repo, so each repo carries its own
//! rules inside that database's `meta` table. A repo that has never had rules
//! saved falls back to the global defaults from `config.toml`, which keeps
//! databases created before per-repo rules existed working unchanged.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::{self, IdGenFn};

pub const KEY_INCLUDE_SUBFIXS: &str = "include_subfixs";
pub const KEY_ID_GEN_FNS: &str = "id_gen_fns";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSettings {
    /// Suffix filter used when scanning this repo. Empty = include every file.
    #[serde(default = "config::default_include_subfixs")]
    pub include_subfixs: Vec<String>,
    /// Rules that derive an item id from a repo-relative path.
    #[serde(default = "config::default_id_gen_fns")]
    pub id_gen_fns: Vec<IdGenFn>,
}

impl RepoSettings {
    /// Lowercase every suffix and drop empty entries; keeps the stored form
    /// comparable so re-saving never produces accidental duplicates.
    pub fn normalize(&mut self) {
        self.include_subfixs = self
            .include_subfixs
            .iter()
            .map(|s| config::normalize_suffix(s))
            .filter(|s| !s.is_empty())
            .collect();
    }
}

/// Read the rules this repo owns.
///
/// Returns the rules plus a flag telling whether the repo has none of its own
/// and is therefore inheriting `fallback` (the global defaults).
pub fn load(conn: &Connection, fallback: &RepoSettings) -> Result<(RepoSettings, bool)> {
    let raw = crate::db::meta_get(conn, KEY_INCLUDE_SUBFIXS);
    let Some(raw) = raw else {
        return Ok((fallback.clone(), true));
    };
    let include_subfixs: Vec<String> =
        serde_json::from_str(&raw).unwrap_or_else(|_| fallback.include_subfixs.clone());
    let id_gen_fns: Vec<IdGenFn> = match crate::db::meta_get(conn, KEY_ID_GEN_FNS) {
        Some(r) => serde_json::from_str(&r).unwrap_or_else(|_| fallback.id_gen_fns.clone()),
        None => fallback.id_gen_fns.clone(),
    };
    Ok((RepoSettings { include_subfixs, id_gen_fns }, false))
}

/// Persist the rules for this repo so it stops inheriting the global defaults.
pub fn save(conn: &Connection, settings: &RepoSettings) -> Result<()> {
    crate::db::meta_set(
        conn,
        KEY_INCLUDE_SUBFIXS,
        &serde_json::to_string(&settings.include_subfixs).context("encode include_subfixs")?,
    )?;
    crate::db::meta_set(
        conn,
        KEY_ID_GEN_FNS,
        &serde_json::to_string(&settings.id_gen_fns).context("encode id_gen_fns")?,
    )
}

/// Drop the repo's own rules so it inherits the global defaults again.
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM meta WHERE key IN (?1, ?2)",
        rusqlite::params![KEY_INCLUDE_SUBFIXS, KEY_ID_GEN_FNS],
    )
    .context("clear repo settings")?;
    Ok(())
}

/// Reject rules that would fail at scan time (bad regex, empty split,
/// unclosed template) so a typo surfaces on save instead of mid-scan.
pub fn validate(settings: &RepoSettings) -> Result<()> {
    for s in &settings.include_subfixs {
        if s.trim().is_empty() {
            anyhow::bail!("include_subfixs contains an empty entry");
        }
    }
    for f in &settings.id_gen_fns {
        if let Some(re) = &f.match_re {
            regex::Regex::new(re).with_context(|| format!("invalid match regex: {re}"))?;
        }
        if f.split.is_empty() {
            anyhow::bail!("id_gen_fns.split must not be empty");
        }
        if !crate::idgen::template_ok(&f.template) {
            anyhow::bail!("invalid id_gen template: {}", f.template);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RepoSettings {
        RepoSettings {
            include_subfixs: vec![".rs".to_string(), "go".to_string()],
            id_gen_fns: vec![IdGenFn {
                match_re: Some("src/".to_string()),
                split: "_".to_string(),
                template: "${arr[1]||arr[0]}".to_string(),
            }],
        }
    }

    #[test]
    fn test_normalize_lower_and_dedot() {
        let mut s = sample();
        s.normalize();
        assert_eq!(s.include_subfixs, vec![".rs".to_string(), ".go".to_string()]);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let s = sample();
        save(&conn, &s).unwrap();

        let (loaded, using_global) = load(&conn, &config::default_repo_settings()).unwrap();
        assert!(!using_global, "保存过规则后不应再继承全局默认");
        assert_eq!(loaded.include_subfixs, s.include_subfixs);
        assert_eq!(loaded.id_gen_fns.len(), 1);
        assert_eq!(loaded.id_gen_fns[0].split, "_");
        assert_eq!(loaded.id_gen_fns[0].match_re.as_deref(), Some("src/"));
    }

    #[test]
    fn test_load_falls_back_to_global_and_clear_restores_it() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let global = config::default_repo_settings();

        let (loaded, using_global) = load(&conn, &global).unwrap();
        assert!(using_global, "全新仓库应继承全局默认");
        assert_eq!(loaded.include_subfixs, global.include_subfixs);

        save(&conn, &sample()).unwrap();
        let (_, using_global) = load(&conn, &global).unwrap();
        assert!(!using_global);

        clear(&conn).unwrap();
        let (loaded, using_global) = load(&conn, &global).unwrap();
        assert!(using_global, "clear 之后应恢复继承全局默认");
        assert_eq!(loaded.include_subfixs, global.include_subfixs);
    }

    #[test]
    fn test_each_repo_has_isolated_rules() {
        let a = Connection::open_in_memory().unwrap();
        let b = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&a).unwrap();
        crate::db::init_schema(&b).unwrap();

        let mut sa = sample();
        sa.normalize();
        save(&a, &sa).unwrap();

        let global = config::default_repo_settings();
        let (loaded_a, using_a) = load(&a, &global).unwrap();
        let (loaded_b, using_b) = load(&b, &global).unwrap();

        assert!(!using_a);
        assert!(using_b, "另一个仓库不应被影响");
        assert_ne!(loaded_a.include_subfixs, loaded_b.include_subfixs);
    }

    #[test]
    fn test_validate_rejects_bad_rules() {
        let mut s = sample();
        s.normalize();
        assert!(validate(&s).is_ok());

        s.id_gen_fns[0].match_re = Some("(".to_string());
        assert!(validate(&s).is_err(), "非法正则应被拒绝");

        s.id_gen_fns[0].match_re = None;
        s.id_gen_fns[0].template = "${arr[0}".to_string();
        assert!(validate(&s).is_err(), "未闭合模板应被拒绝");

        s.id_gen_fns[0].template = "${arr[0]||arr[1]}".to_string();
        s.id_gen_fns[0].split = String::new();
        assert!(validate(&s).is_err(), "空的 split 应被拒绝");
    }
}
