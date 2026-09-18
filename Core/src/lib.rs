mod config;
mod db;
mod ffi;
mod scan;
mod search;
mod text;

pub use config::{Config, Rules};
pub use search::{FileRecord, Page, Query};

use db::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub total: u64,
    pub pending: u64,
    pub rebuilding: bool,
    pub inaccessible: u64,
    pub revision: u64,
    pub inbox_cursor: u64,
    pub applied_cursor: u64,
    pub error: Option<String>,
    pub current_path: String,
}
#[derive(Serialize)]
struct UnavailableFolder {
    path: String,
    message: String,
}

struct Message {
    request: Value,
    reply: mpsc::Sender<std::result::Result<Value, String>>,
}

// The writer owns its SQLite connection on one dedicated thread. Search opens
// independent read-only WAL snapshots and never waits behind a filesystem scan.
pub struct Engine {
    path: PathBuf,
    sender: mpsc::Sender<Message>,
    status: Arc<Mutex<Status>>,
    cancellation: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    _lock: File,
}
impl Engine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path.with_extension("sqlite.lock"))?;
        lock.try_lock()
            .map_err(|_| "This index is already open in another EverythingMac instance.")?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        let mut db = db::open(&path, false)?;
        db::initialize(&db)?;
        let mut config: Option<Config> = db::meta(&db, "config")?
            .map(|s| serde_json::from_str(&s))
            .transpose()?;
        if let Some(config) = &mut config {
            config.normalize_exclusions();
            config.validate()?;
            let tx = db.transaction()?;
            if db::meta(&tx, "exclusion_purge_version")?.as_deref() != Some("2") {
                let removed = scan::purge_excluded_index(&tx, config)?;
                db::adjust_file_count(&tx, -(removed as i64))?;
                db::set_meta(&tx, "exclusion_purge_version", "2")?;
            }
            scan::prune_excluded_failures(&tx, config)?;
            tx.commit()?;
        }
        let status = Arc::new(Mutex::new(Status::default()));
        refresh_status(&db, &status)?;
        let stop = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = mpsc::channel::<Message>();
        let s = status.clone();
        let stopping = stop.clone();
        let handle = thread::Builder::new()
            .name("everything-index".into())
            .spawn(move || worker(db, receiver, s, stopping, config))?;
        Ok(Self {
            path,
            sender,
            status,
            cancellation,
            stop,
            thread: Some(handle),
            _lock: lock,
        })
    }
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    pub fn cancel_search(&self) {
        self.cancellation.fetch_add(1, Ordering::Relaxed);
    }
    pub fn search(&self, query: Query) -> Result<Page> {
        search::search(&self.path, query, self.cancellation.clone())
    }
    fn unavailable_folders(&self) -> Result<Vec<UnavailableFolder>> {
        let db = db::open(&self.path, true)?;
        let mut statement = db.prepare("SELECT path,message FROM scan_errors ORDER BY path")?;
        Ok(statement
            .query_map([], |row| {
                Ok(UnavailableFolder {
                    path: row.get(0)?,
                    message: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn request(&self, request: Value) -> std::result::Result<Value, String> {
        match request["op"].as_str() {
            Some("status") => serde_json::to_value(self.status()).map_err(|e| e.to_string()),
            Some("unavailable") => {
                serde_json::to_value(self.unavailable_folders().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            }
            Some("search") => {
                let query =
                    serde_json::from_value(request).map_err(|e| format!("Invalid search: {e}"))?;
                serde_json::to_value(self.search(query).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            }
            Some("configure" | "events" | "retry" | "checkpoint") => {
                let (tx, rx) = mpsc::channel();
                self.sender
                    .send(Message { request, reply: tx })
                    .map_err(|_| "Index worker has stopped")?;
                rx.recv().map_err(|_| "Index worker has stopped")?
            }
            _ => Err("Unknown core operation".into()),
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel_search();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn refresh_status(db: &Connection, state: &Mutex<Status>) -> Result<()> {
    let pending: i64 = db.query_row(
        "SELECT count(*) FROM pending_dirs WHERE retry_at<=?1",
        [scan::now()],
        |r| r.get(0),
    )?;
    let inaccessible: i64 = db.query_row("SELECT count(*) FROM scan_errors", [], |r| r.get(0))?;
    let inbox = db::meta(db, "inbox_cursor")?
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let applied = db::meta(db, "applied_cursor")?
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let mut status = state.lock().unwrap_or_else(|p| p.into_inner());
    status.total = db::file_count(db)?;
    status.pending = pending as u64;
    status.rebuilding = pending > 0 && db::meta(db, "rebuilding")?.as_deref() == Some("1");
    status.inaccessible = inaccessible as u64;
    status.inbox_cursor = inbox;
    status.applied_cursor = applied;
    Ok(())
}

fn command(db: &mut Connection, request: Value, config: &mut Option<Config>) -> Result<Value> {
    match request["op"].as_str().unwrap_or_default() {
        "configure" => {
            let mut new: Config = serde_json::from_value(request["config"].clone())?;
            new.normalize_exclusions();
            new.validate()?;
            let rebuild = request["rebuild"].as_bool().unwrap_or(false);
            let changed = config.as_ref() != Some(&new);
            let first = config.is_none();
            let tx = db.transaction()?;
            let mut removed = 0;
            let mut added = 0;
            if changed || rebuild {
                db::set_meta(&tx, "rebuilding", "1")?;
                if rebuild {
                    tx.execute("DELETE FROM pending_dirs", [])?;
                    tx.execute("DELETE FROM scan_errors", [])?;
                }
                removed += scan::purge_excluded_index(&tx, &new)?;
                let old_roots: Vec<(i64, String)> = {
                    let mut stmt = tx.prepare("SELECT id,name FROM files WHERE parent=0")?;
                    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for (id, path) in old_roots {
                    if !new.roots.contains(&path) {
                        removed += db::delete_tree(&tx, id)?;
                    }
                }
                for root in &new.roots {
                    if !new.excluded_path(root) {
                        added += i64::from(scan::ensure_root(&tx, root)?);
                    }
                    db::queue(&tx, root, true)?;
                }
            }
            if first {
                let baseline = request["baseline"].as_u64().unwrap_or(0).to_string();
                db::set_meta(&tx, "inbox_cursor", &baseline)?;
                db::set_meta(&tx, "applied_cursor", &baseline)?;
            }
            db::adjust_file_count(&tx, added - removed as i64)?;
            scan::prune_excluded_failures(&tx, &new)?;
            db::set_meta(&tx, "exclusion_purge_version", "2")?;
            db::set_meta(&tx, "config", &serde_json::to_string(&new)?)?;
            tx.commit()?;
            *config = Some(new);
            return Ok(json!({"ok":true,"changed":changed || removed > 0}));
        }
        "events" => {
            let config = config
                .as_ref()
                .ok_or("Configure the index before sending events")?;
            let changes = request["changes"].as_array().ok_or("Missing changes")?;
            let actionable: Vec<_> = changes
                .iter()
                .filter(|change| {
                    change["path"]
                        .as_str()
                        .is_some_and(|path| config.includes(path) && !config.excluded_path(path))
                })
                .collect();
            // Ignore our own DB/WAL and other excluded directories without
            // writing a new cursor: checkpointing self-generated events would
            // create an endless write -> FSEvents -> write feedback loop.
            if actionable.is_empty() && !request["reset"].as_bool().unwrap_or(false) {
                return Ok(json!({"ok":true}));
            }
            let tx = db.transaction()?;
            if request["reset"].as_bool().unwrap_or(false) {
                db::set_meta(&tx, "rebuilding", "1")?;
                for root in &config.roots {
                    db::queue(&tx, root, true)?;
                }
            } else {
                for change in actionable {
                    let path = change["path"].as_str().ok_or("Missing event path")?;
                    if config.includes(path) {
                        let deep = change["recursive"].as_bool().unwrap_or(false);
                        db::queue(&tx, path, deep)?;
                        if deep && config.roots.iter().any(|root| root == path) {
                            db::set_meta(&tx, "rebuilding", "1")?;
                        }
                    }
                }
            }
            // Cursor and inbox are committed together. A restart can safely
            // replay after this cursor because unfinished work is already on disk.
            if let Some(cursor) = request["cursor"].as_u64().filter(|c| *c > 0) {
                let old = db::meta(&tx, "inbox_cursor")?
                    .unwrap_or_default()
                    .parse::<u64>()
                    .unwrap_or(0);
                let cursor = if request["reset"].as_bool().unwrap_or(false) {
                    cursor
                } else {
                    old.max(cursor)
                };
                db::set_meta(&tx, "inbox_cursor", &cursor.to_string())?;
            }
            tx.commit()?;
        }
        "retry" => {
            let tx = db.transaction()?;
            if let Some(config) = config {
                scan::prune_excluded_failures(&tx, config)?;
            }
            tx.execute("UPDATE pending_dirs SET retry_at=0 WHERE retry_at<>0", [])?;
            tx.commit()?;
        }
        "checkpoint" => {
            db.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
        }
        _ => return Err("Unknown write operation".into()),
    }
    Ok(json!({"ok":true}))
}
// Bound commit frequency by directory count, row changes and elapsed time.
// FULL durability is retained; a crash rolls back the whole unfinished batch.
fn scan_batch(
    db: &mut Connection,
    first: (String, bool),
    config: &Config,
    stop: &AtomicBool,
    state: &Mutex<Status>,
) -> Result<scan::Change> {
    let tx = db.transaction()?;
    let started = Instant::now();
    let before = tx.total_changes();
    let mut change = scan::Change::default();
    let mut job = Some(first);
    for _ in 0..128 {
        let Some((path, deep)) = job else { break };
        state.lock().unwrap_or_else(|p| p.into_inner()).current_path = path.clone();
        let next = scan::process(&tx, &path, deep, config, stop)?;
        change.delta += next.delta;
        change.visible |= next.visible;
        if stop.load(Ordering::Relaxed)
            || started.elapsed() >= Duration::from_millis(100)
            || tx.total_changes() - before >= 8_000
        {
            break;
        }
        job = tx
            .query_row(
                "SELECT path,recursive FROM pending_dirs WHERE retry_at<=?1 ORDER BY path LIMIT 1",
                [scan::now()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
    }
    db::adjust_file_count(&tx, change.delta)?;
    tx.commit()?;
    Ok(change)
}

fn worker(
    mut db: Connection,
    receiver: mpsc::Receiver<Message>,
    state: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
    mut config: Option<Config>,
) {
    let mut refreshed = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        // At most 32 messages before giving filesystem work a turn.
        for _ in 0..32 {
            let Ok(message) = receiver.try_recv() else {
                break;
            };
            let configured = message.request["op"] == "configure";
            let result = command(&mut db, message.request, &mut config).map_err(|e| e.to_string());
            if configured && result.is_ok() {
                let mut status = state.lock().unwrap_or_else(|p| p.into_inner());
                if result.as_ref().is_ok_and(|r| r["changed"] == true) {
                    status.revision = status.revision.wrapping_add(1);
                }
            }
            let _ = refresh_status(&db, &state);
            let _ = message.reply.send(result);
        }
        let job: Result<Option<(String, bool)>> = (|| {
            Ok(db.query_row(
            "SELECT path,recursive FROM pending_dirs WHERE retry_at<=?1 ORDER BY path LIMIT 1",
            [scan::now()], |r|Ok((r.get(0)?,r.get(1)?))).optional()?)
        })();
        match job {
            Ok(Some((path, deep))) if config.is_some() => {
                {
                    state.lock().unwrap_or_else(|p| p.into_inner()).current_path = path.clone();
                }
                match scan_batch(
                    &mut db,
                    (path, deep),
                    config.as_ref().unwrap(),
                    &stop,
                    &state,
                ) {
                    Ok(change) => {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.total = s.total.saturating_add_signed(change.delta);
                        if change.visible {
                            s.revision = s.revision.wrapping_add(1);
                        }
                        s.error = None;
                    }
                    Err(e) => {
                        state.lock().unwrap_or_else(|p| p.into_inner()).error = Some(e.to_string());
                        thread::sleep(Duration::from_millis(200));
                    }
                }
            }
            Ok(_) => {
                // Delayed failed folders are not an active rebuild. Their
                // later retry must not reactivate a whole-index progress UI.
                let _ = db.execute(
                    "UPDATE meta SET value='0' WHERE key='rebuilding' AND value<>'0'",
                    [],
                );
                let pending: i64 = db
                    .query_row("SELECT count(*) FROM pending_dirs", [], |r| r.get(0))
                    .unwrap_or(1);
                if pending == 0 {
                    let _=db.execute("UPDATE meta SET value=(SELECT value FROM meta WHERE key='inbox_cursor')
                        WHERE key='applied_cursor' AND value<>(SELECT value FROM meta WHERE key='inbox_cursor')",[]);
                }
                let _ = refresh_status(&db, &state);
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .current_path
                    .clear();
                match receiver.recv_timeout(Duration::from_millis(200)) {
                    Ok(message) => {
                        let configured = message.request["op"] == "configure";
                        let result = command(&mut db, message.request, &mut config)
                            .map_err(|e| e.to_string());
                        if configured && result.is_ok() {
                            let mut status = state.lock().unwrap_or_else(|p| p.into_inner());
                            if result.as_ref().is_ok_and(|r| r["changed"] == true) {
                                status.revision = status.revision.wrapping_add(1);
                            }
                        }
                        let _ = refresh_status(&db, &state);
                        let _ = message.reply.send(result);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
            }
            Err(e) => {
                state.lock().unwrap_or_else(|p| p.into_inner()).error = Some(e.to_string());
                thread::sleep(Duration::from_millis(200));
            }
        }
        if refreshed.elapsed() >= Duration::from_secs(1) {
            let _ = refresh_status(&db, &state);
            refreshed = Instant::now();
        }
    }
    let _ = db.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
}
