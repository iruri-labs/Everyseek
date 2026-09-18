use everything_core::{Engine, Query};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

struct Fixture {
    base: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "everything-core-{}-{}-{}",
            std::process::id(),
            serial,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = base.join("files");
        fs::create_dir_all(&root).unwrap();
        Self { base, root }
    }
    fn open(&self) -> Engine {
        Engine::open(self.base.join("index.sqlite")).unwrap()
    }
    fn configure(&self, e: &Engine) {
        e.request(
            json!({"op":"configure","baseline":100,"config":{"roots":[self.root],"rules":{}}}),
        )
        .unwrap();
    }
    fn event(&self, e: &Engine, cursor: u64, deep: bool) {
        e.request(
            json!({"op":"events","cursor":cursor,"changes":[{"path":self.root,"recursive":deep}]}),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
fn settled(e: &Engine) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = e.status();
        assert!(status.error.is_none(), "{status:?}");
        if status.pending == 0 {
            assert!(!status.rebuilding);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Index did not settle: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn names(e: &Engine, text: &str) -> Vec<String> {
    e.search(Query {
        text: text.into(),
        ..Query::default()
    })
    .unwrap()
    .rows
    .into_iter()
    .map(|r| r.name)
    .collect()
}

#[test]
fn unicode_exact_filtering_and_paging() {
    let f = Fixture::new();
    let decomposed: String = "사업계획서.pdf".nfd().collect();
    for name in [
        &decomposed,
        "회의.pdf",
        "계.txt",
        "가나X나다.txt",
        "가나나다.txt",
        "사업 계획.txt",
        "A계획-1.txt",
        "a계획-2.txt",
    ] {
        fs::write(f.root.join(name), "x").unwrap();
    }
    let e = f.open();
    f.configure(&e);
    settled(&e);
    assert_eq!(names(&e, "계획").len(), 4);
    assert_eq!(names(&e, &"계획".nfd().collect::<String>()).len(), 4);
    assert_eq!(names(&e, "계").len(), 5);
    assert!(names(&e, "ㄱㅎ").is_empty());
    assert_eq!(names(&e, "??.pdf"), ["회의.pdf"]);
    assert_eq!(names(&e, "\"사업 계획\""), ["사업 계획.txt"]);
    assert_eq!(names(&e, "가나나다"), ["가나나다.txt"]);
    let q = Query {
        text: "가나나다".into(),
        limit: 1,
        ..Query::default()
    };
    let page = e.search(q).unwrap();
    assert_eq!(page.rows[0].name, "가나나다.txt");
    assert!(!page.has_more);
    assert_eq!(
        e.search(Query {
            text: "A계획".into(),
            case_insensitive: false,
            ..Query::default()
        })
        .unwrap()
        .rows
        .len(),
        1
    );
    assert_eq!(
        e.search(Query {
            text: "계획".into(),
            whole_word: true,
            ..Query::default()
        })
        .unwrap()
        .rows
        .len(),
        1
    );
    let first = e
        .search(Query {
            text: "계획".into(),
            limit: 2,
            ..Query::default()
        })
        .unwrap();
    let second = e
        .search(Query {
            text: "계획".into(),
            limit: 2,
            offset: 2,
            ..Query::default()
        })
        .unwrap();
    assert!(first.has_more);
    assert!(!second.has_more);
    assert!(
        first
            .rows
            .iter()
            .all(|r| second.rows.iter().all(|s| r.id != s.id))
    );
    let original = e
        .search(Query {
            text: "사업계획서".into(),
            ..Query::default()
        })
        .unwrap()
        .rows
        .remove(0);
    assert_eq!(original.name.as_bytes(), decomposed.as_bytes());
    assert!(std::path::Path::new(&original.path).exists());
    assert!(Engine::open(f.base.join("index.sqlite")).is_err()); // one writer per DB
    let revision = e.status().revision;
    f.event(&e, 105, true);
    settled(&e);
    assert_eq!(e.status().revision, revision); // unchanged scan must not refresh UI
    let rules: everything_core::Rules =
        serde_json::from_value(json!({"excludeVCSFolders": false})).unwrap();
    assert!(!rules.exclude_vcs_folders);
}

#[test]
fn incremental_metadata_move_delete_and_durable_restart() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("옛폴더")).unwrap();
    fs::write(f.root.join("옛폴더/회의.txt"), "x").unwrap();
    let e = f.open();
    f.configure(&e);
    settled(&e);
    fs::rename(f.root.join("옛폴더"), f.root.join("새폴더")).unwrap();
    fs::write(f.root.join("새폴더/회의.txt"), "longer content").unwrap();
    f.event(&e, 101, false);
    settled(&e);
    let q = Query {
        text: "새폴더 회의".into(),
        match_path: true,
        ..Query::default()
    };
    let row = e.search(q).unwrap().rows.remove(0);
    assert_eq!(row.size, 14);
    assert!(names(&e, "옛폴더").is_empty());
    for i in 0..30 {
        let dir = f.root.join(format!("복구{i}"));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("계획.txt"), "x").unwrap();
    }
    f.event(&e, 102, true);
    drop(e); // stop during queued recursive work, reopen without resending it
    let e = f.open();
    settled(&e);
    assert_eq!(names(&e, "계획").len(), 30);
    assert_eq!(e.status().inbox_cursor, 102);
    fs::remove_dir_all(f.root.join("새폴더")).unwrap();
    f.event(&e, 103, false);
    settled(&e);
    assert!(names(&e, "회의").is_empty());
    // Dropped events must schedule a full scan, even for a watched root.
    fs::write(f.root.join("유실복구.txt"), "x").unwrap();
    e.request(json!({"op":"events","cursor":104,"reset":true,"changes":[]}))
        .unwrap();
    settled(&e);
    assert_eq!(names(&e, "유실복구").len(), 1);
}

#[test]
fn permission_failure_preserves_records_and_can_retry() {
    let f = Fixture::new();
    let dir = f.root.join("제한");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("보존.txt"), "x").unwrap();
    let e = f.open();
    f.configure(&e);
    settled(&e);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
    f.event(&e, 101, true);
    settled(&e);
    assert_eq!(names(&e, "보존").len(), 1);
    assert_eq!(e.status().inaccessible, 1);
    let db = rusqlite::Connection::open(f.base.join("index.sqlite")).unwrap();
    let deadline = || {
        db.query_row(
            "SELECT retry_at FROM pending_dirs WHERE path=?1",
            [dir.to_str().unwrap()],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    let before = deadline();
    f.event(&e, 102, true); // ordinary events must respect failed-folder cooldown
    settled(&e);
    assert_eq!(deadline(), before);
    assert_eq!(e.status().inaccessible, 1);
    e.request(json!({"op":"retry"})).unwrap(); // repeated failure backs off to an hour
    settled(&e);
    assert!(deadline() >= before + 3290);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    e.request(json!({"op":"retry"})).unwrap();
    settled(&e);
    assert_eq!(e.status().inaccessible, 0);
    assert_eq!(names(&e, "보존").len(), 1);
}

#[test]
fn exclusions_are_not_failures_and_old_retry_jobs_are_removed() {
    let f = Fixture::new();
    let excluded = f.root.join("제외");
    fs::create_dir(&excluded).unwrap();
    fs::write(excluded.join("보존.txt"), "x").unwrap();
    let e = f.open();
    f.configure(&e);
    settled(&e);
    // Listing names works, but stat of any child fails without search permission.
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o400)).unwrap();
    f.event(&e, 101, true);
    settled(&e);
    assert_eq!(e.status().inaccessible, 1);
    e.request(json!({"op":"configure","config":{"roots":[f.root],
        "rules":{"pathPrefixes":[excluded.to_str().unwrap().nfd().collect::<String>()]}}}))
        .unwrap();
    settled(&e);
    let failures = e.status().inaccessible;
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(failures, 0); // excluded children must be skipped BEFORE stat
    assert!(names(&e, "보존").is_empty());
    drop(e);
    // Simulate a stale failure from an older build, without an indexed dirs row.
    let db = rusqlite::Connection::open(f.base.join("index.sqlite")).unwrap();
    let stale = excluded.join("old-job").to_str().unwrap().to_owned();
    db.execute("INSERT INTO scan_errors VALUES(?1,'old failure')", [&stale])
        .unwrap();
    db.execute(
        "INSERT INTO pending_dirs VALUES(?1,1,9223372036854775807)",
        [&stale],
    )
    .unwrap();
    let e = f.open();
    assert_eq!(e.status().inaccessible, 0);
    e.request(json!({"op":"retry"})).unwrap();
    settled(&e);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pending_dirs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(e.status().inaccessible, 0);
    f.configure(&e); // include the on-disk folder again
    settled(&e);
    let revision = e.status().revision;
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o000)).unwrap();
    e.request(json!({"op":"configure","config":{"roots":[f.root],
        "rules":{"pathPrefixes":[excluded]}}}))
        .unwrap();
    let removed = names(&e, "보존").is_empty();
    let updated = e.status().revision > revision;
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        removed && updated,
        "Apply must purge the DB before any filesystem rescan"
    );
    e.request(json!({"op":"configure","config":{"roots":[f.root],
        "rules":{"pathPrefixes":[f.root]}}}))
        .unwrap();
    settled(&e);
    assert_eq!(e.status().total, 0); // explicit root exclusions also take effect
}
