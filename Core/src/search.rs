use crate::{
    db::{self, Result},
    text,
};
use rusqlite::{functions::FunctionFlags, params};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub text: String,
    pub match_path: bool,
    pub case_insensitive: bool,
    pub whole_word: bool,
    pub sort: String,
    pub ascending: bool,
    pub limit: usize,
    pub offset: usize,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            text: String::new(),
            match_path: false,
            case_insensitive: true,
            whole_word: false,
            sort: "name".into(),
            ascending: true,
            limit: 500,
            offset: 0,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: i64,
    pub parent: i64,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub is_dir: bool,
    pub vol_id: i64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub rows: Vec<FileRecord>,
    pub has_more: bool,
}

pub fn search(path: &Path, query: Query, cancellation: Arc<AtomicU64>) -> Result<Page> {
    let db = db::open(path, true)?;
    let generation = cancellation.load(Ordering::Relaxed);
    let cancel = cancellation.clone();
    db.progress_handler(
        1000,
        Some(move || cancel.load(Ordering::Relaxed) != generation),
    )?;
    let terms = text::terms(&query.text);
    // Case-insensitive grams are only a candidate filter. Exact case and word
    // boundaries are enforced before ORDER BY / LIMIT, including paged queries.
    let candidate = if query.match_path {
        None
    } else {
        text::candidate_query(&terms)
    };
    let patterns: Vec<_> = terms
        .iter()
        .map(|s| text::normalize(s, query.case_insensitive))
        .collect();
    db.create_scalar_function(
        "em_matches",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let raw: String = ctx.get(0)?;
            let text = text::normalize(&raw, query.case_insensitive);
            Ok(patterns
                .iter()
                .all(|p| text::matches(p, &text, query.whole_word)))
        },
    )?;
    let full_path = "CASE WHEN f.parent=0 THEN f.name WHEN d.path='/' THEN '/'||f.name ELSE d.path||'/'||f.name END";
    let order = if query.ascending { "ASC" } else { "DESC" };
    let sort = match query.sort.as_str() {
        "size" => format!("f.size {order},f.name_key {order}"),
        "mtime" => format!("f.mtime {order},f.name_key {order}"),
        "path" => format!("path {order}"),
        "kind" => format!(
            "f.is_dir {},f.ext {order},f.name_key {order}",
            if query.ascending { "DESC" } else { "ASC" }
        ),
        _ => format!("f.name_key {order}"),
    };
    let from = if candidate.is_some() {
        "JOIN name_fts ON name_fts.rowid=f.id"
    } else {
        ""
    };
    let condition = if candidate.is_some() {
        "name_fts MATCH ?1 AND"
    } else {
        "?1='' AND"
    };
    let target = if query.match_path {
        full_path
    } else {
        "f.name"
    };
    let sql = if terms.is_empty() && query.sort != "path" {
        // The initial browser page needs no text filter. Each supported sort
        // has a matching index: stop at limit + 1 before looking up paths.
        // Materialization prevents the directory join from changing that plan.
        format!(
            "WITH page AS MATERIALIZED (
                SELECT f.* FROM files f WHERE ?1=''
                ORDER BY {sort},f.id {order} LIMIT ?2 OFFSET ?3
            )
            SELECT f.id,f.parent,f.name,{full_path} AS path,f.size,f.mtime,f.is_dir,f.device
            FROM page f LEFT JOIN dirs d ON d.id=f.parent
            ORDER BY {sort},f.id {order}"
        )
    } else {
        // Filter all candidates before LIMIT so matches and paging stay exact.
        format!(
            "SELECT f.id,f.parent,f.name,{full_path} AS path,f.size,f.mtime,f.is_dir,f.device
            FROM files f LEFT JOIN dirs d ON d.id=f.parent {from}
            WHERE {condition} em_matches({target})
            ORDER BY {sort},f.id {order} LIMIT ?2 OFFSET ?3"
        )
    };
    let limit = query.limit.clamp(1, 10_000);
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            candidate.unwrap_or_default(),
            (limit + 1) as i64,
            query.offset.min(i64::MAX as usize) as i64
        ],
        |r| {
            Ok(FileRecord {
                id: r.get(0)?,
                parent: r.get(1)?,
                name: r.get(2)?,
                path: r.get(3)?,
                size: r.get(4)?,
                mtime: r.get(5)?,
                is_dir: r.get(6)?,
                vol_id: r.get(7)?,
            })
        },
    )?;
    let mut rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if cancellation.load(Ordering::Relaxed) != generation {
        return Err("Search cancelled".into());
    }
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    Ok(Page { rows, has_more })
}
