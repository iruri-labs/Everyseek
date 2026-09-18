use crate::{
    config::{Config, MARKERS, SCOPED},
    db::{self, Result},
    text,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct Change {
    pub delta: i64,
    pub visible: bool,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}
pub fn insert(db: &Connection, parent: i64, name: &str, metadata: &fs::Metadata) -> Result<i64> {
    let key = text::normalize(name, true);
    let is_dir = metadata.is_dir();
    let ext = if is_dir {
        String::new()
    } else {
        Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| text::normalize(s, true))
            .unwrap_or_default()
    };
    db.execute(
        "INSERT INTO files(parent,name,name_key,ext,size,mtime,mtime_ns,is_dir,device,inode)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            parent,
            name,
            key,
            ext,
            metadata.size() as i64,
            metadata.mtime(),
            metadata.mtime_nsec(),
            is_dir,
            metadata.dev() as i64,
            metadata.ino() as i64
        ],
    )?;
    let id = db.last_insert_rowid();
    db.execute(
        "INSERT INTO name_fts(rowid,tokens) VALUES(?1,?2)",
        params![id, text::tokens(name)],
    )?;
    Ok(id)
}
pub fn ensure_root(db: &Connection, path: &str) -> Result<bool> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM dirs WHERE path=?1)",
        [path],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(false);
    }
    // An unavailable root remains a durable job; never pretend it was empty.
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.is_dir()
    {
        let id = insert(db, 0, path, &metadata)?;
        db.execute("INSERT INTO dirs VALUES(?1,?2)", params![id, path])?;
        return Ok(true);
    }
    Ok(false)
}
fn inaccessible(db: &Connection, path: &str, error: impl std::fmt::Display) -> Result<Change> {
    let repeated: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM scan_errors WHERE path=?1)",
        [path],
        |r| r.get(0),
    )?;
    // One retry after five minutes, then hourly while the failure persists.
    let delay = if repeated { 3600 } else { 300 };
    db.execute("INSERT INTO scan_errors VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET message=excluded.message",
        params![path,error.to_string()])?;
    db.execute(
        "UPDATE pending_dirs SET retry_at=?2 WHERE path=?1",
        params![path, now() + delay],
    )?;
    Ok(Change::default())
}
fn missing(db: &Connection, path: &str) -> Result<Change> {
    let id: Option<i64> = db
        .query_row("SELECT id FROM dirs WHERE path=?1", [path], |r| r.get(0))
        .optional()?;
    let delta = if let Some(id) = id {
        -(db::delete_tree(db, id)? as i64)
    } else {
        0
    };
    db::clear_jobs_below(db, path)?;
    Ok(Change {
        delta,
        visible: delta != 0,
    })
}

// Upgrade/retry cleanup: an intentional exclusion is never an access failure.
// Inspect only failed paths, not the millions of indexed files.
pub fn prune_excluded_failures(db: &Connection, config: &Config) -> Result<()> {
    let paths: Vec<String> = db
        .prepare("SELECT path FROM scan_errors")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for path in paths {
        if !config.includes(&path) || config.excluded_path(&path) {
            db::clear_jobs_below(db, &path)?;
        }
    }
    Ok(())
}

// Apply exclusions from indexed paths, even when a parent can no longer be read.
// Only directory metadata is scanned; descendants are deleted by indexed IDs.
pub fn purge_excluded_index(db: &Connection, config: &Config) -> Result<usize> {
    let mut excluded = BTreeMap::<String, i64>::new();
    {
        let mut stmt = db.prepare("SELECT d.id,d.path,f.name,f.parent FROM dirs d JOIN files f ON f.id=d.id ORDER BY d.path")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(1)?;
            if Path::new(&path)
                .ancestors()
                .skip(1)
                .any(|p| p.to_str().is_some_and(|p| excluded.contains_key(p)))
            {
                continue;
            }
            let name: String = row.get(2)?;
            let mut skip =
                !config.includes(&path) || config.rules.excluded(&name, &path, true, false);
            if !skip && config.rules.exclude_dev_folders && SCOPED.contains(&name.as_str()) {
                let mut markers = db.prepare_cached("SELECT name FROM files WHERE parent=?1")?;
                let mut names = markers.query([row.get::<_, i64>(3)?])?;
                while let Some(marker) = names.next()? {
                    if MARKERS.contains(&marker.get::<_, String>(0)?.as_str()) {
                        skip = true;
                        break;
                    }
                }
            }
            if skip {
                excluded.insert(path, row.get(0)?);
            }
        }
    }
    // Explicit paths can also identify symlinks, which have no dirs row.
    for path in &config.rules.path_prefixes {
        let p = Path::new(path);
        if let (Some(parent), Some(name)) = (
            p.parent().and_then(Path::to_str),
            p.file_name().and_then(|s| s.to_str()),
        ) {
            let id: Option<i64> = db.query_row("SELECT f.id FROM files f JOIN dirs d ON f.parent=d.id WHERE d.path=?1 AND f.name=?2",
                params![parent, name], |r| r.get(0)).optional()?;
            if let Some(id) = id {
                excluded.insert(path.clone(), id);
            }
        }
        db::clear_jobs_below(db, path)?;
    }
    let mut removed = 0;
    // A parent's old error may have come from inspecting an excluded child.
    // Recheck those ancestors once; keep unrelated failure cooldowns intact.
    let failures: Vec<String> = db
        .prepare("SELECT path FROM scan_errors")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for parent in failures {
        if excluded
            .keys()
            .chain(config.rules.path_prefixes.iter())
            .any(|path| crate::config::within(path, &parent))
        {
            db.execute(
                "UPDATE pending_dirs SET retry_at=0 WHERE path=?1 AND retry_at<>0",
                [parent],
            )?;
        }
    }
    for id in excluded.into_values() {
        removed += db::delete_tree(db, id)?;
    }
    prune_excluded_failures(db, config)?;
    Ok(removed)
}

#[derive(Debug)]
struct Existing {
    id: i64,
    dir: bool,
    device: i64,
    inode: i64,
    size: i64,
    mtime: i64,
    ns: i64,
}

// Called inside the writer batch transaction. File/FTS changes, child jobs and
// job completion remain atomic even when several directories share one commit.
pub fn process(
    db: &Connection,
    path: &str,
    deep: bool,
    config: &Config,
    stop: &AtomicBool,
) -> Result<Change> {
    if !config.includes(path) {
        db::clear_jobs_below(db, path)?;
        return Ok(Change::default());
    }
    if config.excluded_path(path) {
        return missing(db, path);
    }
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_dir() => return missing(db, path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return missing(db, path),
        Err(e) => return inaccessible(db, path, e),
        _ => {}
    }
    let iterator = match fs::read_dir(path) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return missing(db, path),
        Err(e) => return inaccessible(db, path, e),
    };
    let mut children = Vec::new();
    for item in iterator {
        if stop.load(Ordering::Relaxed) {
            return Err("Indexing stopped".into());
        }
        let item = match item {
            Ok(item) => item,
            Err(e) => return inaccessible(db, path, e),
        };
        let name = match item.file_name().into_string() {
            Ok(name) => name,
            Err(_) => {
                return inaccessible(db, path, "Filename is not valid UTF-8; directory preserved");
            }
        };
        children.push((name, item));
    }
    let project = children
        .iter()
        .any(|(name, _)| MARKERS.contains(&name.as_str()));
    let mut entries = Vec::new();
    for (name, item) in children {
        if stop.load(Ordering::Relaxed) {
            return Err("Indexing stopped".into());
        }
        let full = join(path, &name);
        if config.rules.excluded_without_metadata(&name, &full) {
            continue;
        }
        let kind = match item.file_type() {
            Ok(kind) => kind,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return inaccessible(db, path, e),
        };
        if config.rules.excluded(&name, &full, kind.is_dir(), project) {
            continue;
        }
        let metadata = match fs::symlink_metadata(item.path()) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return inaccessible(db, path, e),
        };
        entries.push((name, metadata));
    }
    let had_dir: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM dirs WHERE path=?1)",
        [path],
        |r| r.get(0),
    )?;
    ensure_root_if_configured(db, path, config)?;
    let dir_id: Option<i64> = db
        .query_row("SELECT id FROM dirs WHERE path=?1", [path], |r| r.get(0))
        .optional()?;
    let Some(dir_id) = dir_id else {
        // A new nested directory's event can arrive before its parent. Reconcile
        // from the nearest known ancestor instead of silently dropping it.
        for ancestor in Path::new(path).ancestors().skip(1) {
            let p = ancestor.to_string_lossy();
            if !config.includes(&p) {
                break;
            }
            let known: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM dirs WHERE path=?1)",
                [&*p],
                |r| r.get(0),
            )?;
            if known {
                db::queue(db, &p, true)?;
                break;
            }
        }
        db.execute("DELETE FROM pending_dirs WHERE path=?1", [path])?;
        return Ok(Change::default());
    };
    let mut existing: HashMap<String, Existing> = {
        let mut stmt = db.prepare_cached(
            "SELECT name,id,is_dir,device,inode,size,mtime,mtime_ns FROM files WHERE parent=?1",
        )?;
        stmt.query_map([dir_id], |r| {
            Ok((
                r.get(0)?,
                Existing {
                    id: r.get(1)?,
                    dir: r.get(2)?,
                    device: r.get(3)?,
                    inode: r.get(4)?,
                    size: r.get(5)?,
                    mtime: r.get(6)?,
                    ns: r.get(7)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let mut delta = i64::from(!had_dir);
    let mut visible = !had_dir;
    for (name, metadata) in entries {
        let full = join(path, &name);
        if config
            .rules
            .excluded(&name, &full, metadata.is_dir(), project)
        {
            continue;
        }
        let old = existing.remove(&name);
        let old = if let Some(old) = old {
            if old.dir != metadata.is_dir()
                || old.device != metadata.dev() as i64
                || old.inode != metadata.ino() as i64
            {
                visible = true;
                delta -= db::delete_tree(db, old.id)? as i64;
                None
            } else {
                Some(old)
            }
        } else {
            None
        };
        let fresh = old.is_none();
        let id = if let Some(old) = old {
            if old.size != metadata.size() as i64
                || old.mtime != metadata.mtime()
                || old.ns != metadata.mtime_nsec()
            {
                visible = true;
                db.execute(
                    "UPDATE files SET size=?2,mtime=?3,mtime_ns=?4 WHERE id=?1",
                    params![
                        old.id,
                        metadata.size() as i64,
                        metadata.mtime(),
                        metadata.mtime_nsec()
                    ],
                )?;
            }
            old.id
        } else {
            visible = true;
            delta += 1;
            insert(db, dir_id, &name, &metadata)?
        };
        if metadata.is_dir() {
            db.execute("INSERT INTO dirs(id,path) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET path=excluded.path WHERE dirs.path<>excluded.path", params![id,full])?;
            if deep || fresh {
                db::queue(db, &full, true)?;
            }
        }
    }
    for old in existing.into_values() {
        visible = true;
        delta -= db::delete_tree(db, old.id)? as i64;
    }
    db.execute("DELETE FROM scan_errors WHERE path=?1", [path])?;
    db.execute("DELETE FROM pending_dirs WHERE path=?1", [path])?;
    Ok(Change { delta, visible })
}
fn ensure_root_if_configured(db: &Connection, path: &str, config: &Config) -> Result<()> {
    if config.roots.iter().any(|r| r == path) {
        ensure_root(db, path)?;
    }
    Ok(())
}
