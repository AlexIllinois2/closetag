use crate::config::{self, Config};
use crate::db::DbPool;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU16;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: Mutex<Config>,
    /// One SQLite pool per repo, keyed by repo slug.
    pub dbs: Mutex<HashMap<String, DbPool>>,
    pub ws: Arc<WsTracker>,
    pub started: Instant,
    pub port: AtomicU16,
}

/// Tracks connected web pages so the server can exit when the page closes.
pub struct WsTracker {
    inner: Mutex<WsInner>,
}

struct WsInner {
    clients: usize,
    ever_connected: bool,
    last_disconnect: Option<Instant>,
}

const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

impl WsTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(WsInner {
                clients: 0,
                ever_connected: false,
                last_disconnect: None,
            }),
        }
    }

    pub fn on_connect(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.clients += 1;
        inner.ever_connected = true;
        inner.last_disconnect = None;
    }

    pub fn on_disconnect(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.clients > 0 {
            inner.clients -= 1;
        }
        if inner.clients == 0 {
            inner.last_disconnect = Some(Instant::now());
        }
    }

    /// True when the process should exit: the page was open and then closed
    /// (after a grace period).
    pub fn should_exit(&self, grace: Duration) -> bool {
        let inner = self.inner.lock().unwrap();
        if inner.clients > 0 {
            return false;
        }
        if !inner.ever_connected {
            return false; // checked by the caller against a wall-clock deadline
        }
        match inner.last_disconnect {
            Some(t) => t.elapsed() >= grace,
            None => false,
        }
    }

    pub fn ever_connected(&self) -> bool {
        self.inner.lock().unwrap().ever_connected
    }

    pub fn clients(&self) -> usize {
        self.inner.lock().unwrap().clients
    }
}

impl Default for WsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf, cfg: Config) -> Self {
        let mut dbs: HashMap<String, DbPool> = HashMap::new();
        for p in &cfg.repo_paths {
            let slug = config::repo_slug(p);
            let db_path = config::repo_db_path(&data_dir, &slug);
            match crate::db::open_pool(&db_path) {
                Ok(pool) => {
                    dbs.insert(slug, pool);
                }
                Err(e) => eprintln!("[closetag] failed to open db for {p}: {e}"),
            }
        }
        Self {
            data_dir,
            config: Mutex::new(cfg),
            dbs: Mutex::new(dbs),
            ws: Arc::new(WsTracker::new()),
            started: Instant::now(),
            port: AtomicU16::new(0),
        }
    }

    /// Resolve a repo slug to its configured root path.
    pub fn repo_path(&self, repo_id: &str) -> Result<String> {
        let cfg = self.config.lock().unwrap();
        cfg.repo_paths
            .iter()
            .find(|p| config::repo_slug(p) == repo_id)
            .cloned()
            .with_context(|| format!("unknown repo: {repo_id}"))
    }

    /// Get (lazily opening) the database pool for a repo slug.
    pub fn pool_for(&self, repo_id: &str) -> Result<DbPool> {
        let path = self.repo_path(repo_id)?;
        let slug = config::repo_slug(&path);
        {
            let dbs = self.dbs.lock().unwrap();
            if let Some(pool) = dbs.get(&slug) {
                return Ok(pool.clone());
            }
        }
        let db_path = config::repo_db_path(&self.data_dir, &slug);
        let pool = crate::db::open_pool(&db_path)?;
        self.dbs.lock().unwrap().insert(slug, pool.clone());
        Ok(pool)
    }

    /// Drop pools for repos that are no longer configured.
    pub fn prune_dbs(&self) {
        let live: std::collections::HashSet<String> = self
            .config
            .lock()
            .unwrap()
            .repo_paths
            .iter()
            .map(|p| config::repo_slug(p))
            .collect();
        self.dbs
            .lock()
            .unwrap()
            .retain(|k, _| live.contains(k));
    }
}

/// Idle websocket timeout used by the ws handler to detect dead pages.
pub fn ws_idle_timeout() -> Duration {
    WS_IDLE_TIMEOUT
}
