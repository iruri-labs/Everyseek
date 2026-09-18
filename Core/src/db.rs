use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{path::Path, time::Duration};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn open(path: &Path, readonly: bool) -> Result<Connection> {
    let flags = if readonly {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch(
        "PRAGMA cache_size=-16384; PRAGMA temp_store=FILE; PRAGMA mmap_size=67108864;",
    )?;
    if readonly {
        db.execute_batch("PRAGMA query_only=ON;")?;
    }
    Ok(db)
}
pub fn initialize(db: &Connection) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > 2 {
        return Err("This index was created by a newer EverythingMac. Update the app.".into());
    }
    db.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        PRAGMA wal_autocheckpoint=8192; PRAGMA journal_size_limit=33554432;
        PRAGMA cache_size=-65536;",
    )?;
    // Ordinary opens need neither schema writes nor a scan of the files table.
    if version == 2 {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS files(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent INTEGER NOT NULL,
            name TEXT NOT NULL COLLATE BINARY,
            name_key TEXT NOT NULL,
            ext TEXT NOT NULL,
            size INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            mtime_ns INTEGER NOT NULL,
            is_dir INTEGER NOT NULL,
            device INTEGER NOT NULL,
            inode INTEGER NOT NULL,
            UNIQUE(parent,name)
        );
        CREATE INDEX IF NOT EXISTS files_name ON files(name_key,id);
        DROP INDEX IF EXISTS files_size;
        DROP INDEX IF EXISTS files_mtime;
        CREATE INDEX IF NOT EXISTS files_size_sort ON files(size,name_key,id);
        CREATE INDEX IF NOT EXISTS files_mtime_sort ON files(mtime,name_key,id);
        CREATE INDEX IF NOT EXISTS files_kind ON files(is_dir DESC,ext,name_key,id);
        CREATE TABLE IF NOT EXISTS dirs(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
        CREATE VIRTUAL TABLE IF NOT EXISTS name_fts USING fts5(
            tokens, content='', contentless_delete=1, detail=none, tokenize='ascii'
        );
        CREATE TABLE IF NOT EXISTS pending_dirs(
            path TEXT PRIMARY KEY, recursive INTEGER NOT NULL, retry_at INTEGER NOT NULL DEFAULT 0
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS scan_errors(path TEXT PRIMARY KEY, message TEXT NOT NULL) WITHOUT ROWID;
        INSERT OR IGNORE INTO meta VALUES('inbox_cursor','0'),('applied_cursor','0'),('rebuilding','0');",
    )?;
    // Seed once for old databases. Subsequent changes update this in the same
    // transaction as the corresponding files, including exclusions and roots.
    let total: i64 = tx.query_row("SELECT count(*) FROM files", [], |r| r.get(0))?;
    set_meta(&tx, "file_count", &total.to_string())?;
    tx.execute_batch("PRAGMA user_version=2;")?;
    tx.commit()?;
    Ok(())
}
pub fn meta(db: &Connection, key: &str) -> Result<Option<String>> {
    Ok(db
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
        .optional()?)
}
pub fn set_meta(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute(
        "INSERT INTO meta VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE meta.value<>excluded.value",
        params![key, value],
    )?;
    Ok(())
}
pub fn file_count(db: &Connection) -> Result<u64> {
    Ok(meta(db, "file_count")?
        .ok_or("Missing index file count")?
        .parse()?)
}
pub fn adjust_file_count(db: &Connection, delta: i64) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let updated = db.execute(
        "UPDATE meta SET value=CAST(value AS INTEGER)+?1
         WHERE key='file_count' AND CAST(value AS INTEGER)+?1>=0",
        [delta],
    )?;
    if updated != 1 {
        return Err("Invalid index file count".into());
    }
    Ok(())
}
pub fn queue(db: &Connection, path: &str, deep: bool) -> Result<()> {
    db.execute(
        "INSERT INTO pending_dirs(path,recursive) VALUES(?1,?2)
        ON CONFLICT(path) DO UPDATE SET recursive=max(recursive,excluded.recursive)
        WHERE pending_dirs.recursive<excluded.recursive",
        params![path, deep],
    )?;
    Ok(())
}
pub fn delete_tree(db: &Connection, id: i64) -> Result<usize> {
    if let Some(path) = db
        .query_row("SELECT path FROM dirs WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        // Old failed jobs may have no dirs row. Clear the entire path subtree.
        clear_jobs_below(db, &path)?;
    }
    db.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS doomed(id INTEGER PRIMARY KEY); DELETE FROM doomed;",
    )?;
    db.execute(
        "INSERT INTO doomed WITH RECURSIVE tree(id) AS (
        SELECT ?1 UNION ALL SELECT f.id FROM files f JOIN tree ON f.parent=tree.id
        ) SELECT id FROM tree",
        [id],
    )?;
    db.execute(
        "DELETE FROM name_fts WHERE rowid IN (SELECT id FROM doomed)",
        [],
    )?;
    for table in ["pending_dirs", "scan_errors"] {
        db.execute(
            &format!(
                "DELETE FROM {table} WHERE path IN
            (SELECT path FROM dirs WHERE id IN (SELECT id FROM doomed))"
            ),
            [],
        )?;
    }
    db.execute("DELETE FROM dirs WHERE id IN (SELECT id FROM doomed)", [])?;
    let deleted = db.execute("DELETE FROM files WHERE id IN (SELECT id FROM doomed)", [])?;
    Ok(deleted)
}

pub fn clear_jobs_below(db: &Connection, path: &str) -> Result<()> {
    let prefix = format!("{}/", path.trim_end_matches('/'));
    for table in ["pending_dirs", "scan_errors"] {
        db.execute(
            &format!("DELETE FROM {table} WHERE path=?1 OR substr(path,1,length(?2))=?2"),
            params![path, prefix],
        )?;
    }
    Ok(())
}
