use crate::config::{normalize_suffix, InfoParseFn};
use crate::db;
use crate::infoparse;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use walkdir::WalkDir;

/// Directories that are never scanned (in addition to any dot-directory).
pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".output",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    ".closetag",
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanReport {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub duration_ms: u128,
}

/// Sync the `items` table with the files on disk:
/// - new files are inserted (name/auth parsed via info-parse rules)
/// - items whose parsed (name, auth) changed are updated
/// - items whose file disappeared are deleted (tag links cascade away)
pub fn scan_repo(
    root: &Path,
    include_subfixs: &[String],
    rules: &[InfoParseFn],
    conn: &Connection,
) -> Result<ScanReport> {
    let started = Instant::now();
    let suffixes: Vec<String> = include_subfixs
        .iter()
        .map(|s| normalize_suffix(s))
        .filter(|s| !s.is_empty())
        .collect();

    // existing items: path -> (id, name, auth)
    let mut existing: HashMap<String, (i64, String, String)> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name, auth, path FROM items")
            .context("prepare items read")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .context("query items")?;
        for row in rows {
            let (id, name, auth, path) = row?;
            existing.insert(path, (id, name, auth));
        }
    }

    let mut seen: HashMap<String, (String, String)> = HashMap::new(); // path -> (name, auth)
    let mut added = 0usize;
    let mut updated = 0usize;

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true; // always scan the repo root itself
            }
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                if name.starts_with('.') {
                    return false;
                }
                if SKIP_DIRS.contains(&name.as_ref()) {
                    return false;
                }
            }
            true
        });

    let tx = conn.unchecked_transaction().context("begin scan tx")?;
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = match entry.path().strip_prefix(root) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if !suffixes.is_empty() {
            let ext = format!(
                ".{}",
                rel.extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default()
            );
            if !suffixes.contains(&ext) {
                continue;
            }
        }
        let info = infoparse::parse_info(&rel_str, rules);
        match existing.get(&rel_str) {
            Some((id, old_name, old_auth)) => {
                if old_name != &info.name || old_auth != &info.auth {
                    tx.execute(
                        "UPDATE items SET name = ?1, auth = ?2, updated_at = ?3 WHERE id = ?4",
                        rusqlite::params![info.name, info.auth, db::now_iso(), id],
                    )
                    .context("update item name/auth")?;
                    updated += 1;
                }
            }
            None => {
                tx.execute(
                    "INSERT INTO items(name, auth, path, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?4)",
                    rusqlite::params![info.name, info.auth, rel_str, db::now_iso()],
                )
                .context("insert item")?;
                added += 1;
            }
        }
        seen.insert(rel_str, (info.name, info.auth));
    }

    let mut removed = 0usize;
    for (path, (id, _, _)) in existing.iter() {
        if !seen.contains_key(path) {
            tx.execute("DELETE FROM items WHERE id = ?1", [id])
                .context("delete missing item")?;
            removed += 1;
        }
    }
    tx.commit().context("commit scan tx")?;

    Ok(ScanReport {
        scanned: seen.len(),
        added,
        updated,
        removed,
        duration_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoParseFn;
    use rusqlite::Connection;
    use std::fs;

    fn rule(split: &str, name: &str, auth: &str) -> InfoParseFn {
        InfoParseFn {
            match_re: None,
            split: split.to_string(),
            name: name.to_string(),
            auth: auth.to_string(),
        }
    }

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("closetag-scan-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("创建临时目录");
        p
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn test_scan_skips_ignored_dirs_and_unwanted_suffixes() {
        let root = tmpdir("skip");
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::create_dir_all(root.join("node_modules")).expect("mkdir node_modules");
        fs::create_dir_all(root.join(".hidden")).expect("mkdir .hidden");
        fs::write(root.join("src/date-helper.ts"), "a").unwrap();
        fs::write(root.join("src/date-parser.ts"), "b").unwrap();
        fs::write(root.join("README.md"), "c").unwrap();
        fs::write(root.join("node_modules/skip.ts"), "d").unwrap(); // 忽略目录
        fs::write(root.join(".hidden/secret.ts"), "e").unwrap(); // 隐藏目录
        fs::write(root.join("image.png"), "f").unwrap(); // 后缀不在白名单

        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let suffixes = vec![".ts".to_string(), ".md".to_string()];
        let rules = vec![rule(" - ", "${arr[0]}", "${arr[1]}")];

        let report = scan_repo(&root, &suffixes, &rules, &conn).unwrap();
        assert_eq!(report.scanned, 3, "只有 3 个文件应被收录");
        assert_eq!(report.added, 3);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 3);

        let (name, auth): (String, String) = conn
            .query_row(
                "SELECT name, auth FROM items WHERE path = 'src/date-helper.ts'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "date-helper", "name 应由 info_parse 规则从文件名解析");
        assert_eq!(auth, "", "文件名无第二段时 auth 为空");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_adds_updates_removes() {
        let root = tmpdir("delta");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/old.ts"), "a").unwrap();
        fs::write(root.join("src/keep.ts"), "b").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let suffixes = vec![".ts".to_string()];
        let rules = vec![rule(" - ", "${arr[0]}", "${arr[1]}")];

        let first = scan_repo(&root, &suffixes, &rules, &conn).unwrap();
        assert_eq!(first.added, 2);

        // 删掉一个、新增一个、另一个不动
        fs::remove_file(root.join("src/old.ts")).unwrap();
        fs::write(root.join("src/new.ts"), "c").unwrap();
        let second = scan_repo(&root, &suffixes, &rules, &conn).unwrap();
        assert_eq!(second.added, 1, "新增 1 个");
        assert_eq!(second.removed, 1, "移除 1 个");
        assert_eq!(second.updated, 0, "未改动的条目不应刷新");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_updates_name_when_rule_changes() {
        let root = tmpdir("reid");
        fs::write(root.join("a - b.ts"), "x").unwrap();
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let suffixes = vec![".ts".to_string()];

        scan_repo(&root, &suffixes, &[rule(" - ", "${arr[0]}", "${arr[1]}")], &conn).unwrap();
        let (name1, auth1): (String, String) = conn
            .query_row(
                "SELECT name, auth FROM items WHERE path = 'a - b.ts'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name1, "a");
        assert_eq!(auth1, "b");

        let report = scan_repo(&root, &suffixes, &[rule(" - ", "${arr[1]}", "${arr[0]}")], &conn).unwrap();
        assert_eq!(report.updated, 1, "name/auth 变化应计入 updated");
        let (name2, auth2): (String, String) = conn
            .query_row(
                "SELECT name, auth FROM items WHERE path = 'a - b.ts'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name2, "b");
        assert_eq!(auth2, "a");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_removes_tag_links_of_deleted_items() {
        let root = tmpdir("cascade");
        fs::write(root.join("gone.ts"), "x").unwrap();
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        scan_repo(&root, &[".ts".to_string()], &[rule(" - ", "${arr[0]}", "")], &conn).unwrap();
        conn.execute("INSERT INTO tags(name, parent_id, description, created_at, updated_at) VALUES ('t', NULL, '', 'x', 'x')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO item_tags(item_id, tag_id) VALUES ((SELECT id FROM items WHERE path = 'gone.ts'), 1)",
            [],
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM item_tags"), 1);

        fs::remove_file(root.join("gone.ts")).unwrap();
        scan_repo(&root, &[".ts".to_string()], &[rule(" - ", "${arr[0]}", "")], &conn).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM item_tags"), 0, "标签关联应级联删除");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tags"), 1, "标签本身要保留");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_empty_suffix_list_means_all_files() {
        let root = tmpdir("all");
        fs::write(root.join("a.ts"), "x").unwrap();
        fs::write(root.join("b.bin"), "y").unwrap();
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let report = scan_repo(&root, &[], &[rule(" - ", "${arr[0]}", "")], &conn).unwrap();
        assert_eq!(report.scanned, 2, "后缀列表为空时收录所有文件");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_duplicate_name_auth_is_allowed() {
        // (name, auth) 不设唯一索引：不同目录下的同名文件（同一作品的变体）可以共存
        let root = tmpdir("dupe");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/Naruto - Kishimoto.cbz"), "x").unwrap();
        fs::write(root.join("b/Naruto - Kishimoto.cbz"), "y").unwrap();
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let rules = vec![rule(" - ", "${arr[0]}", "${arr[1]}")];

        let report = scan_repo(&root, &[".cbz".to_string()], &rules, &conn).unwrap();
        assert_eq!(report.added, 2, "重复的 (name, auth) 不应被唯一索引挡住");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 2);
        let _ = fs::remove_dir_all(&root);
    }
}
