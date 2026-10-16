use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Open (creating if needed) the SQLite database for one repo and make sure
/// the schema exists.
pub fn open_pool(db_path: &Path) -> Result<DbPool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let manager = SqliteConnectionManager::file(db_path).with_init(|conn: &mut Connection| {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;",
        )?;
        Ok(())
    });
    let pool = r2d2::Pool::builder()
        .max_size(4)
        .build(manager)
        .with_context(|| format!("open sqlite db {}", db_path.display()))?;
    {
        let conn = pool.get().context("get connection from pool")?;
        init_schema(&conn)?;
    }
    Ok(pool)
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tags (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            parent_id   INTEGER REFERENCES tags(id) ON DELETE SET NULL,
            description TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tags_parent ON tags(parent_id);
        CREATE TABLE IF NOT EXISTS items (
            pk         INTEGER PRIMARY KEY AUTOINCREMENT,
            id         TEXT NOT NULL,
            path       TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_id ON items(id);
        CREATE TABLE IF NOT EXISTS item_tags (
            item_pk INTEGER NOT NULL REFERENCES items(pk) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (item_pk, tag_id)
        );
        CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);
        CREATE TABLE IF NOT EXISTS tag_groups (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE,
            includes    TEXT,
            excludes    TEXT NOT NULL DEFAULT '[]',
            description TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        "#,
    )
    .context("init sqlite schema")?;
    Ok(())
}

pub fn meta_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
