//! Per-repo rules: the file-suffix filter and the info-parse rules.
//!
//! closetag keeps one SQLite database per repo, so each repo carries its own
//! rules inside that database's `meta` table. A repo that has never had rules
//! saved falls back to the global defaults from `config.toml`, which keeps
//! databases created before per-repo rules existed working unchanged.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::{self, InfoParseFn};

pub const KEY_INCLUDE_SUBFIXS: &str = "include_subfixs";
pub const KEY_INFO_PARSE_FNS: &str = "info_parse_fns";
/// Databases created before `info_parse` existed store their rules under this
/// key; they are migrated on open (see `migrate_legacy_rules`).
pub const LEGACY_KEY_ID_GEN_FNS: &str = "id_gen_fns";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSettings {
    /// Suffix filter used when scanning this repo. Empty = include every file.
    #[serde(default = "config::default_include_subfixs")]
    pub include_subfixs: Vec<String>,
    /// Rules that parse a file name into (name, auth).
    #[serde(default = "config::default_info_parse_fns")]
    pub info_parse_fns: Vec<InfoParseFn>,
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
    let info_parse_fns: Vec<InfoParseFn> = match crate::db::meta_get(conn, KEY_INFO_PARSE_FNS) {
        Some(r) => serde_json::from_str(&r).unwrap_or_else(|_| fallback.info_parse_fns.clone()),
        None => fallback.info_parse_fns.clone(),
    };
    Ok((RepoSettings { include_subfixs, info_parse_fns }, false))
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
        KEY_INFO_PARSE_FNS,
        &serde_json::to_string(&settings.info_parse_fns).context("encode info_parse_fns")?,
    )
}

/// Drop the repo's own rules so it inherits the global defaults again.
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM meta WHERE key IN (?1, ?2)",
        rusqlite::params![KEY_INCLUDE_SUBFIXS, KEY_INFO_PARSE_FNS],
    )
    .context("clear repo settings")?;
    Ok(())
}

/// The old `id_gen_fns` rule shape: a single template rendered against the
/// repo-relative path.
#[derive(Deserialize)]
struct LegacyIdGenFn {
    #[serde(default, rename = "match")]
    match_re: Option<String>,
    #[serde(default = "config::default_split")]
    split: String,
    #[serde(default)]
    template: String,
}

/// One-time migration for databases saved before `info_parse` replaced
/// `id_gen`: each old rule is rewritten as an info-parse rule (the old
/// template becomes the `name` template, `auth` starts empty) and the old key
/// is dropped. Unparsable or empty rule sets are simply removed so the repo
/// inherits the new global defaults.
pub fn migrate_legacy_rules(conn: &Connection) -> Result<()> {
    let Some(raw) = crate::db::meta_get(conn, LEGACY_KEY_ID_GEN_FNS) else {
        return Ok(());
    };
    let converted: Vec<InfoParseFn> = serde_json::from_str::<Vec<LegacyIdGenFn>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.template.trim().is_empty() && !f.split.is_empty())
        .map(|f| InfoParseFn {
            match_re: f.match_re,
            split: f.split,
            name: f.template,
            auth: String::new(),
        })
        .collect();
    if !converted.is_empty() {
        // Write only the info-parse key: `include_subfixs` keeps its own state
        // (a repo may own id rules while still inheriting global suffixes).
        crate::db::meta_set(
            conn,
            KEY_INFO_PARSE_FNS,
            &serde_json::to_string(&converted).context("encode info_parse_fns")?,
        )?;
    }
    conn.execute(
        "DELETE FROM meta WHERE key = ?1",
        rusqlite::params![LEGACY_KEY_ID_GEN_FNS],
    )
    .context("drop legacy id_gen_fns")?;
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
    for f in &settings.info_parse_fns {
        if let Some(re) = &f.match_re {
            regex::Regex::new(re).with_context(|| format!("invalid match regex: {re}"))?;
        }
        if f.split.is_empty() {
            anyhow::bail!("info_parse_fns.split must not be empty");
        }
        if !crate::infoparse::template_ok(&f.name) {
            anyhow::bail!("invalid info_parse name template: {}", f.name);
        }
        if !f.auth.trim().is_empty() && !crate::infoparse::template_ok(&f.auth) {
            anyhow::bail!("invalid info_parse auth template: {}", f.auth);
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
            info_parse_fns: vec![InfoParseFn {
                match_re: Some("src/".to_string()),
                split: "_".to_string(),
                name: "${arr[1]||arr[0]}".to_string(),
                auth: "${arr[0]}".to_string(),
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
        assert_eq!(loaded.info_parse_fns.len(), 1);
        assert_eq!(loaded.info_parse_fns[0].split, "_");
        assert_eq!(loaded.info_parse_fns[0].match_re.as_deref(), Some("src/"));
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

        s.info_parse_fns[0].match_re = Some("(".to_string());
        assert!(validate(&s).is_err(), "非法正则应被拒绝");

        s.info_parse_fns[0].match_re = None;
        s.info_parse_fns[0].name = "${arr[0}".to_string();
        assert!(validate(&s).is_err(), "未闭合模板应被拒绝");

        s.info_parse_fns[0].name = "${arr[0]}".to_string();
        s.info_parse_fns[0].split = String::new();
        assert!(validate(&s).is_err(), "空的 split 应被拒绝");
    }

    #[test]
    fn test_legacy_id_gen_rules_are_migrated() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        // 旧格式：单 template，按路径切分（与 include_subfixs 一起保存）
        crate::db::meta_set(
            &conn,
            LEGACY_KEY_ID_GEN_FNS,
            r#"[{"split":"-","template":"${arr[0]||arr[1]}"}]"#,
        )
        .unwrap();
        crate::db::meta_set(&conn, KEY_INCLUDE_SUBFIXS, r#"[".rs"]"#).unwrap();

        migrate_legacy_rules(&conn).unwrap();

        assert!(crate::db::meta_get(&conn, LEGACY_KEY_ID_GEN_FNS).is_none(), "旧键应被删除");
        let (loaded, using_global) = load(&conn, &config::default_repo_settings()).unwrap();
        assert!(!using_global, "迁移后该仓库拥有自己的规则");
        assert_eq!(loaded.include_subfixs, vec![".rs".to_string()], "后缀规则不受影响");
        assert_eq!(loaded.info_parse_fns.len(), 1);
        assert_eq!(loaded.info_parse_fns[0].name, "${arr[0]||arr[1]}");
        assert_eq!(loaded.info_parse_fns[0].auth, "");
    }

    #[test]
    fn test_legacy_migration_drops_garbage() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::meta_set(&conn, LEGACY_KEY_ID_GEN_FNS, "not-json").unwrap();
        crate::db::meta_set(&conn, KEY_INCLUDE_SUBFIXS, r#"[".rs"]"#).unwrap();

        migrate_legacy_rules(&conn).unwrap();

        let (loaded, using_global) = load(&conn, &config::default_repo_settings()).unwrap();
        assert!(!using_global, "仓库仍拥有自己的后缀规则");
        assert_eq!(
            loaded.info_parse_fns,
            config::default_repo_settings().info_parse_fns,
            "无法解析的旧规则应被丢弃，回退到全局默认的解析规则"
        );
    }
}
