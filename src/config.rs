use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "closetag";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 17733;

/// All app data lives under the user data directory (per-OS convention):
/// Windows: %APPDATA%\closetag, Linux: ~/.local/share/closetag, macOS: ~/Library/Application Support/closetag
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("cannot resolve the user data directory")?;
    Ok(base.join(APP_NAME))
}

pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.toml")
}

pub fn repo_db_path(data_dir: &Path, slug: &str) -> PathBuf {
    data_dir.join("repos").join(format!("{slug}.sqlite"))
}

pub fn default_include_subfixs() -> Vec<String> {
    [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".vue", ".svelte",
        ".rs", ".py", ".go", ".java", ".kt", ".c", ".h", ".cc", ".cpp", ".hpp",
        ".cs", ".rb", ".php", ".swift", ".scala", ".sh", ".sql",
        ".md", ".txt", ".json", ".yaml", ".yml", ".toml", ".xml",
        ".html", ".css", ".scss", ".less",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn default_id_gen_fns() -> Vec<IdGenFn> {
    vec![IdGenFn {
        match_re: None,
        split: "-".to_string(),
        template: "${arr[0]||arr[1]}".to_string(),
    }]
}

impl Config {
    /// The rules a newly added repo starts with.
    ///
    /// Suffixes and id-gen rules are stored per repo (see `repo_settings`), so
    /// the values here only act as a template applied at "add repo" time.
    pub fn repo_defaults(&self) -> crate::repo_settings::RepoSettings {
        crate::repo_settings::RepoSettings {
            include_subfixs: self.include_subfixs.clone(),
            id_gen_fns: self.id_gen_fns.clone(),
        }
    }
}

/// The rules a brand new repo inherits before anything is customised.
pub fn default_repo_settings() -> crate::repo_settings::RepoSettings {
    crate::repo_settings::RepoSettings {
        include_subfixs: default_include_subfixs(),
        id_gen_fns: default_id_gen_fns(),
    }
}

/// Normalize a suffix entry: lowercase, ensure a leading dot.
pub fn normalize_suffix(s: &str) -> String {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() {
        return t;
    }
    if t.starts_with('.') {
        t
    } else {
        format!(".{t}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdGenFn {
    /// Optional regex matched against the repo-relative path; the rule only applies when it matches.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "match")]
    pub match_re: Option<String>,
    /// Delimiter used to split the relative path into `arr`.
    #[serde(default = "default_split")]
    pub split: String,
    /// Template evaluated against `arr`, e.g. "${arr[0]||arr[1]}".
    #[serde(default)]
    pub template: String,
}

fn default_split() -> String {
    "-".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// 0 = pick a free port automatically.
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub open_browser: bool,
    #[serde(default = "default_true")]
    pub exit_when_closed: bool,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_true() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            open_browser: true,
            exit_when_closed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Repo roots; each repo gets its own SQLite database.
    #[serde(default)]
    pub repo_paths: Vec<String>,
    /// Default suffix filter handed to a newly added repo; each repo can then
    /// override it. Empty = include every file.
    #[serde(default = "default_include_subfixs")]
    pub include_subfixs: Vec<String>,
    /// Default id rules handed to a newly added repo; each repo can then
    /// override them.
    #[serde(default = "default_id_gen_fns")]
    pub id_gen_fns: Vec<IdGenFn>,
    #[serde(default)]
    pub server: ServerConfig,
}

impl Default for Config {
    fn default() -> Self {
        let rules = default_repo_settings();
        Self {
            repo_paths: Vec::new(),
            include_subfixs: rules.include_subfixs,
            id_gen_fns: rules.id_gen_fns,
            server: ServerConfig::default(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        for r in &self.repo_paths {
            if r.trim().is_empty() {
                anyhow::bail!("repo_paths contains an empty entry");
            }
        }
        crate::repo_settings::validate(&self.repo_defaults())
    }
}

/// Load config from disk, creating a default config.toml on first run.
pub fn load(data_dir: &Path) -> Result<Config> {
    let path = config_path(data_dir);
    if path.exists() {
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        // Be lenient on load so a single bad rule never bricks the UI; the
        // config page re-validates on save.
        if let Err(e) = cfg.validate() {
            eprintln!("[closetag] warning: config is invalid: {e}");
        }
        Ok(cfg)
    } else {
        let cfg = Config::default();
        save(data_dir, &cfg)?;
        Ok(cfg)
    }
}

pub fn save(data_dir: &Path, cfg: &Config) -> Result<()> {
    cfg.validate()?;
    fs::create_dir_all(data_dir).with_context(|| format!("create {}", data_dir.display()))?;
    let text = toml::to_string_pretty(cfg).context("serialize config")?;
    fs::write(config_path(data_dir), text).with_context(|| "write config.toml")?;
    Ok(())
}

/// Stable slug for a repo path, e.g. "closetag-3f2a9c".
pub fn repo_slug(path: &str) -> String {
    let p = Path::new(path);
    let raw = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c == ' ' {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    if name.trim_matches('_').is_empty() {
        name = "root".to_string();
    }
    let h = fnv1a(path.as_bytes());
    format!("{}-{:06x}", name.to_lowercase(), h & 0xFF_FFFF)
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Canonicalize a path string for storage: forward slashes, no \\?\ prefix,
/// no trailing slash (but keep drive roots like "C:/").
pub fn canonical_display(path: &str) -> String {
    let s = path.trim().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    let trimmed = s.trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        s
    } else if trimmed.len() == 2 && trimmed.ends_with(':') {
        s
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_slug_is_stable_and_unique() {
        let a = repo_slug("D:/weiheng/proj/closetag");
        assert_eq!(a, repo_slug("D:/weiheng/proj/closetag"), "同一路径 slug 必须稳定");
        assert_ne!(a, repo_slug("D:/weiheng/proj/other"), "不同路径 slug 必须不同");
        assert!(a.starts_with("closetag-"), "slug 应带可读的目录名: {a}");
        assert!(a.len() > "closetag-".len(), "slug 应带哈希后缀: {a}");
    }

    #[test]
    fn test_repo_slug_sanitizes_names() {
        let s = repo_slug("D:/我的 项目/a+b");
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "slug 只含安全字符: {s}");
    }

    #[test]
    fn test_canonical_display() {
        assert_eq!(canonical_display(r"C:\Users\x\repo"), "C:/Users/x/repo");
        assert_eq!(canonical_display("//?/C:/Users/x/repo"), "C:/Users/x/repo", "应去掉 \\\\?\\ 前缀");
        assert_eq!(canonical_display("C:/repo/"), "C:/repo");
        assert_eq!(canonical_display("C:/"), "C:/", "盘符根目录要保留斜杠");
        assert_eq!(canonical_display("/"), "/");
    }

    #[test]
    fn test_normalize_suffix() {
        assert_eq!(normalize_suffix(".TS"), ".ts");
        assert_eq!(normalize_suffix("rs"), ".rs");
        assert_eq!(normalize_suffix("  .py  "), ".py");
        assert_eq!(normalize_suffix(""), "");
    }

    #[test]
    fn test_config_roundtrip() {
        let dir = std::env::temp_dir().join(format!("closetag-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config::default();
        save(&dir, &cfg).expect("保存配置");
        let loaded = load(&dir).expect("加载配置");
        assert_eq!(loaded.server.port, DEFAULT_PORT);
        assert_eq!(loaded.id_gen_fns.len(), 1);
        assert_eq!(loaded.id_gen_fns[0].split, "-");
        assert!(loaded.include_subfixs.contains(&".rs".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_rejects_bad_rules() {
        let mut cfg = Config::default();
        cfg.id_gen_fns[0].match_re = Some("(".to_string());
        assert!(cfg.validate().is_err(), "非法正则应被拒绝");

        cfg.id_gen_fns[0].match_re = None;
        cfg.id_gen_fns[0].template = "${arr[0}".to_string();
        assert!(cfg.validate().is_err(), "未闭合模板应被拒绝");

        cfg.id_gen_fns[0].template = "${arr[0]||arr[1]}".to_string();
        cfg.id_gen_fns[0].split = "".to_string();
        assert!(cfg.validate().is_err(), "空的 split 应被拒绝");

        cfg.id_gen_fns[0].split = "-".to_string();
        assert!(cfg.validate().is_ok(), "正常配置应通过");
    }
}
