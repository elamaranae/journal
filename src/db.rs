use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::entry::{Entry, Mood};

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(journal_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(journal_path)?;
        let db_path = journal_path.join("journal.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {}", db_path.display()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS entries (
                id          TEXT PRIMARY KEY,
                title       TEXT,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                tags        TEXT NOT NULL DEFAULT '',
                mood        TEXT,
                word_count  INTEGER NOT NULL DEFAULT 0
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts
            USING fts5(id UNINDEXED, title, body, tokenize='porter ascii');

            CREATE TABLE IF NOT EXISTS entries_body (
                id   TEXT PRIMARY KEY,
                body TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert(&self, entry: &Entry) -> Result<()> {
        let tags = entry.tags.join(",");
        let mood = entry.mood.as_ref().map(|m| m.to_string());

        self.conn.execute(
            "INSERT OR REPLACE INTO entries (id, title, created_at, updated_at, tags, mood, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.id,
                entry.title,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
                tags,
                mood,
                entry.word_count() as i64,
            ],
        )?;

        self.conn.execute(
            "INSERT OR REPLACE INTO entries_body (id, body) VALUES (?1, ?2)",
            params![entry.id, entry.body],
        )?;

        // FTS5: delete old row by rowid lookup, then re-insert
        self.conn.execute(
            "DELETE FROM entries_fts WHERE rowid IN (SELECT rowid FROM entries_fts WHERE id = ?1)",
            params![entry.id],
        )?;
        self.conn.execute(
            "INSERT INTO entries_fts (id, title, body) VALUES (?1, ?2, ?3)",
            params![entry.id, entry.title.as_deref().unwrap_or(""), entry.body],
        )?;

        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        self.conn.execute("DELETE FROM entries_body WHERE id = ?1", params![id])?;
        self.conn.execute(
            "DELETE FROM entries_fts WHERE rowid IN (SELECT rowid FROM entries_fts WHERE id = ?1)",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_by_id(&self, id_prefix: &str) -> Result<Option<EntryRow>> {
        let pattern = format!("{}%", id_prefix);
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.title, e.created_at, e.updated_at, e.tags, e.mood, e.word_count, b.body
             FROM entries e LEFT JOIN entries_body b ON e.id = b.id
             WHERE e.id LIKE ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![pattern], row_to_entry_row)?;
        Ok(rows.next().transpose()?)
    }

    pub fn list(
        &self,
        tag_filter: Option<&str>,
        from: Option<DateTime<Local>>,
        to: Option<DateTime<Local>>,
        limit: usize,
    ) -> Result<Vec<EntryRow>> {
        // Build parameterized WHERE clause — never interpolate user values into SQL.
        let mut clauses: Vec<&str> = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        let tag_param;
        if let Some(tag) = tag_filter {
            clauses.push("(',' || tags || ',') LIKE ?");
            tag_param = format!("%,{},%", tag);
            param_values.push(tag_param.clone());
        }
        if let Some(f) = from {
            clauses.push("created_at >= ?");
            param_values.push(f.to_rfc3339());
        }
        if let Some(t) = to {
            clauses.push("created_at <= ?");
            param_values.push(t.to_rfc3339());
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        // LIMIT is an integer we control — not user-supplied text, safe to format.
        let limit_clause = if limit == usize::MAX {
            String::new()
        } else {
            format!("LIMIT {}", limit)
        };

        let sql = format!(
            "SELECT e.id, e.title, e.created_at, e.updated_at, e.tags, e.mood, e.word_count, b.body
             FROM entries e LEFT JOIN entries_body b ON e.id = b.id
             {} ORDER BY e.created_at DESC {}",
            where_clause, limit_clause
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(param_values.iter()), row_to_entry_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn search(&self, query: &str) -> Result<Vec<EntryRow>> {
        // 1. Case-insensitive LIKE on title and body (fuzzy partial match).
        let like_pat = format!("%{}%", query);
        let like_sql =
            "SELECT e.id, e.title, e.created_at, e.updated_at, e.tags, e.mood, e.word_count, b.body
             FROM entries e LEFT JOIN entries_body b ON e.id = b.id
             WHERE LOWER(e.title) LIKE LOWER(?1) OR LOWER(COALESCE(b.body,'')) LIKE LOWER(?1)
             ORDER BY e.created_at DESC";
        let like_rows: Vec<EntryRow> = self
            .conn
            .prepare(like_sql)?
            .query_map(params![like_pat], row_to_entry_row)?
            .filter_map(|r| r.ok())
            .collect();

        // 2. FTS5 full-text search (relevance-ranked). Quote the query so special
        //    characters don't break the FTS5 parser. Errors are silently ignored —
        //    LIKE results are still returned.
        let fts_query = format!("\"{}\"", query.replace('"', " "));
        let fts_sql =
            "SELECT e.id, e.title, e.created_at, e.updated_at, e.tags, e.mood, e.word_count, b.body
             FROM entries_fts f
             JOIN entries e ON f.id = e.id
             LEFT JOIN entries_body b ON e.id = b.id
             WHERE entries_fts MATCH ?1
             ORDER BY rank";
        let fts_rows: Vec<EntryRow> = self
            .conn
            .prepare(fts_sql)
            .and_then(|mut s| {
                Ok(s.query_map(params![fts_query], row_to_entry_row)?
                    .filter_map(|r| r.ok())
                    .collect())
            })
            .unwrap_or_default();

        // Merge: FTS results first (relevance-ranked), then LIKE-only matches.
        let mut seen: std::collections::HashSet<String> =
            fts_rows.iter().map(|r| r.id.clone()).collect();
        let mut result = fts_rows;
        for row in like_rows {
            if seen.insert(row.id.clone()) {
                result.push(row);
            }
        }
        Ok(result)
    }

    pub fn count(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn total_words(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COALESCE(SUM(word_count),0) FROM entries", [], |r| r.get(0))?;
        Ok(n)
    }
}

#[derive(Debug, Clone)]
pub struct EntryRow {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Local>,
    pub tags: Vec<String>,
    pub mood: Option<Mood>,
    pub word_count: i64,
    pub body: Option<String>,
}

fn row_to_entry_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRow> {
    let id: String = row.get(0)?;
    let title: Option<String> = row.get(1)?;
    let created_str: String = row.get(2)?;
    let tags_str: String = row.get(4)?;
    let mood_str: Option<String> = row.get(5)?;
    let word_count: i64 = row.get(6)?;
    let body: Option<String> = row.get(7)?;

    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map(|d| d.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now());

    let tags = if tags_str.is_empty() {
        vec![]
    } else {
        tags_str.split(',').map(|s| s.to_string()).collect()
    };

    let mood = mood_str.as_deref().and_then(|m| m.parse::<Mood>().ok());

    Ok(EntryRow { id, title, created_at, tags, mood, word_count, body })
}
