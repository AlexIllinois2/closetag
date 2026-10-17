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
    // Table creation is idempotent; on an old database `items`/`item_tags`
    // keep their previous shape until `migrate_items_v2` rebuilds them, which
    // is why the new indexes are only created afterwards.
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
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL,
            auth       TEXT NOT NULL DEFAULT '',
            path       TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS item_tags (
            item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (item_id, tag_id)
        );
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
    migrate_items_v2(conn).context("migrate items schema")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_name_auth ON items(name, auth);
         CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);",
    )
    .context("init sqlite indexes")?;
    crate::repo_settings::migrate_legacy_rules(conn).context("migrate legacy id_gen rules")?;
    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let mut stmt = match conn.prepare(&format!("PRAGMA table_info({table})")) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let rows = stmt.query_map([], |r| r.get::<_, String>(1));
    match rows {
        Ok(rows) => rows.flatten().any(|name| name == column),
        Err(_) => false,
    }
}

/// Rebuild databases written before `items.id` became an INTEGER primary key.
///
/// The old table kept an internal `pk` rowid next to a TEXT `id` generated
/// from the path; the new table drops `pk`, makes `id` the autoincrement
/// primary key, and adds `name`/`auth`. Every row keeps its old `pk` as the
/// new `id` so `item_tags` links survive untouched, and the old generated id
/// string is copied into `name` (the next scan refreshes it from the new
/// info-parse rules).
fn migrate_items_v2(conn: &Connection) -> Result<()> {
    if !has_column(conn, "items", "pk") {
        return Ok(()); // fresh database or already migrated
    }
    // Foreign keys must be toggled outside a transaction, and the rebuild
    // drops/recreates both `items` and `item_tags`. The batch carries its own
    // BEGIN/COMMIT because `Connection::transaction` needs `&mut self`.
    conn.execute_batch("PRAGMA foreign_keys=OFF;")
        .context("disable foreign keys for migration")?;
    let result = conn
        .execute_batch(
            r#"
            BEGIN;
            CREATE TABLE items_new (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL,
                auth       TEXT NOT NULL DEFAULT '',
                path       TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO items_new(id, name, auth, path, created_at, updated_at)
                SELECT pk, id, '', path, created_at, updated_at FROM items;
            DROP TABLE items;
            ALTER TABLE items_new RENAME TO items;
            DELETE FROM sqlite_sequence WHERE name = 'items';
            INSERT INTO sqlite_sequence(name, seq)
                VALUES('items', (SELECT COALESCE(MAX(id), 0) FROM items));
            CREATE TABLE item_tags_new (
                item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (item_id, tag_id)
            );
            INSERT INTO item_tags_new(item_id, tag_id)
                SELECT item_pk, tag_id FROM item_tags;
            DROP TABLE item_tags;
            ALTER TABLE item_tags_new RENAME TO item_tags;
            COMMIT;
            "#,
        )
        .context("migrate items schema");
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .context("re-enable foreign keys after migration")?;
    result
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
