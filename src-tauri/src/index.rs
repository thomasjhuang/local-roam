//! Index — the SQLite mirror of the vault.
//!
//! The vault (Markdown files) is always the source of truth; this index is a cache
//! for fast queries (backlinks, search) and the home of recall *telemetry*
//! (`last_recalled`, `recall_strength`). Knowledge lives in the files; if this index
//! is lost it can be rebuilt with [`Index::rebuild_from_vault`] — only usage stats,
//! not knowledge, are lost.

use crate::vault::{Note, Vault};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

/// Days of non-exercise after which an edge is considered fully faded (decay == 1.0).
/// The thesis: a connection you haven't retrieved in a month has likely faded from
/// memory, so the UI fades it too and demands a fresh justification to restore it.
const DECAY_HORIZON_DAYS: f64 = 30.0;

/// How faded an edge is on a 0.0 (fresh) … 1.0 (fully decayed) scale, from the time
/// elapsed since it was last *exercised* — recalled, restored, or (at creation)
/// justified. An edge with no exercise timestamp is treated as fully decayed.
pub fn decay(last_recalled: Option<&str>, now: DateTime<Utc>) -> f64 {
    let Some(ts) = last_recalled else { return 1.0 };
    let Ok(then) = DateTime::parse_from_rfc3339(ts) else {
        return 1.0;
    };
    let elapsed_days = (now - then.with_timezone(&Utc)).num_seconds() as f64 / 86_400.0;
    (elapsed_days / DECAY_HORIZON_DAYS).clamp(0.0, 1.0)
}

/// Spaced-repetition schedule. Each net-successful recall roughly doubles the wait
/// before a connection is surfaced for review again (SM-2-style geometric backoff):
/// well-known links are quizzed rarely, shaky ones often.
const REVIEW_BASE_DAYS: f64 = 1.0;
const REVIEW_EASE: f64 = 2.0;

/// Days to wait before re-quizzing an edge of the given recall strength.
pub fn review_interval_days(recall_strength: f64) -> f64 {
    REVIEW_BASE_DAYS * REVIEW_EASE.powf(recall_strength.max(0.0))
}

/// Whether an edge is due for a spaced-repetition review now. A never-exercised edge
/// (no timestamp) is always due.
pub fn is_due(recall_strength: f64, last_recalled: Option<&str>, now: DateTime<Utc>) -> bool {
    review_overdue_ratio(recall_strength, last_recalled, now) >= 1.0
}

/// How overdue an edge is as a multiple of its interval: 1.0 = exactly due, >1.0 =
/// overdue, <1.0 = not yet due, ∞ = never exercised. Ranks the review queue.
fn review_overdue_ratio(
    recall_strength: f64,
    last_recalled: Option<&str>,
    now: DateTime<Utc>,
) -> f64 {
    let Some(ts) = last_recalled else {
        return f64::INFINITY;
    };
    let Ok(then) = DateTime::parse_from_rfc3339(ts) else {
        return f64::INFINITY;
    };
    let elapsed_days = (now - then.with_timezone(&Utc)).num_seconds() as f64 / 86_400.0;
    elapsed_days / review_interval_days(recall_strength)
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NodeMeta {
    pub id: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Backlink {
    pub from_id: String,
    pub from_title: String,
    pub why: String,
    pub recall_strength: f64,
    /// RFC3339 of the last time this edge was exercised, or null if unknown.
    pub last_recalled: Option<String>,
    /// 0.0 (fresh) … 1.0 (fully faded). See [`decay`].
    pub decay: f64,
}

/// An edge due for a spaced-repetition review. Carries only the endpoints — the
/// justification is withheld until the user commits a self-graded answer, preserving
/// recall-before-reveal.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DueReview {
    pub from_id: String,
    pub from_title: String,
    pub to_id: String,
    pub to_title: String,
    pub recall_strength: f64,
    pub last_recalled: Option<String>,
}

/// A connection the user keeps failing to recall, for the "what to review" surface.
/// Carries the endpoints and failure stats but *not* the justification — it points you
/// at a weak spot to review, it is not a cheat sheet.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct FailedConnection {
    pub from_id: String,
    pub from_title: String,
    pub to_id: String,
    pub to_title: String,
    pub failures: i64,
    pub attempts: i64,
}

/// A tag and how many notes carry it, for the tag-browsing escape hatch (#18c).
/// Navigation only — like search, it points you at existing notes; it never creates
/// a note or an edge.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct OutLink {
    pub to_id: String,
    pub to_title: String,
    pub why: String,
    /// RFC3339 of the last time this edge was exercised, or null if unknown.
    pub last_recalled: Option<String>,
    /// 0.0 (fresh) … 1.0 (fully faded). See [`decay`].
    pub decay: f64,
}

pub struct Index {
    conn: Connection,
}

impl Index {
    pub fn open_in_memory() -> Result<Self> {
        let idx = Self {
            conn: Connection::open_in_memory()?,
        };
        idx.migrate()?;
        Ok(idx)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let idx = Self {
            conn: Connection::open(path)?,
        };
        idx.migrate()?;
        Ok(idx)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id      TEXT PRIMARY KEY,
                title   TEXT NOT NULL,
                aliases TEXT NOT NULL DEFAULT '[]',
                refs    TEXT NOT NULL DEFAULT '[]',
                tags    TEXT NOT NULL DEFAULT '[]',
                body    TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS edges (
                from_id         TEXT NOT NULL,
                to_id           TEXT NOT NULL,
                justification   TEXT NOT NULL,
                last_recalled   TEXT,
                recall_strength REAL NOT NULL DEFAULT 0,
                PRIMARY KEY (from_id, to_id)
            );
            CREATE TABLE IF NOT EXISTS recall_log (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                from_id TEXT NOT NULL,
                to_id   TEXT NOT NULL,
                success INTEGER NOT NULL,
                ts      TEXT NOT NULL
            );",
        )?;
        // Back-fill the tags column on indexes created before #18c. Idempotent: the
        // column is added only if missing, so re-opening an already-migrated index is a
        // no-op. The tags themselves repopulate on the next `rebuild_from_vault`.
        if !self.column_exists("nodes", "tags")? {
            self.conn
                .execute("ALTER TABLE nodes ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'", [])?;
        }
        Ok(())
    }

    /// Whether `table` already has a column named `column`. Used to make schema
    /// back-fills idempotent.
    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let found = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        Ok(found)
    }

    /// Sync the index to match the vault, preserving recall telemetry on edges that
    /// still exist. Idempotent: running it twice on an unchanged vault is a no-op.
    pub fn rebuild_from_vault(&self, vault: &Vault) -> Result<()> {
        let notes = vault.list_notes()?;
        let keep_ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();

        for note in &notes {
            self.reindex_note(note)?;
        }

        // Drop nodes that no longer exist on disk.
        let placeholders = std::iter::repeat("?")
            .take(keep_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let params = rusqlite::params_from_iter(keep_ids.iter());
        if keep_ids.is_empty() {
            self.conn.execute("DELETE FROM nodes", [])?;
            self.conn.execute("DELETE FROM edges", [])?;
        } else {
            self.conn.execute(
                &format!("DELETE FROM nodes WHERE id NOT IN ({placeholders})"),
                params,
            )?;
            self.conn.execute(
                &format!("DELETE FROM edges WHERE from_id NOT IN ({placeholders})"),
                rusqlite::params_from_iter(keep_ids.iter()),
            )?;
        }
        Ok(())
    }

    /// Upsert a single note's node row and its outgoing edges, preserving recall
    /// telemetry on edges that already existed.
    pub fn reindex_note(&self, note: &Note) -> Result<()> {
        self.conn.execute(
            "INSERT INTO nodes (id, title, aliases, refs, tags, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, aliases=excluded.aliases,
                refs=excluded.refs, tags=excluded.tags, body=excluded.body",
            rusqlite::params![
                note.id,
                note.title,
                serde_json::to_string(&note.aliases)?,
                serde_json::to_string(&note.refs)?,
                serde_json::to_string(&note.tags)?,
                note.body,
            ],
        )?;

        // Upsert edges; keep recall stats on conflict (only justification changes).
        // A brand-new edge is stamped exercised *now* — justifying a link is itself an
        // act of retrieval, so a fresh edge starts un-faded and only decays from here.
        let now = Utc::now().to_rfc3339();
        let keep_targets: Vec<String> = note.links.iter().map(|l| l.to.clone()).collect();
        for link in &note.links {
            self.conn.execute(
                "INSERT INTO edges (from_id, to_id, justification, last_recalled)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(from_id, to_id) DO UPDATE SET justification=excluded.justification",
                rusqlite::params![note.id, link.to, link.why, now],
            )?;
        }
        // Remove edges this note no longer authors.
        if keep_targets.is_empty() {
            self.conn
                .execute("DELETE FROM edges WHERE from_id=?1", [&note.id])?;
        } else {
            let ph = std::iter::repeat("?")
                .take(keep_targets.len())
                .collect::<Vec<_>>()
                .join(",");
            let mut args: Vec<&dyn rusqlite::ToSql> = vec![&note.id];
            for t in &keep_targets {
                args.push(t);
            }
            self.conn.execute(
                &format!("DELETE FROM edges WHERE from_id=?1 AND to_id NOT IN ({ph})"),
                args.as_slice(),
            )?;
        }
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM nodes WHERE id=?1", [id])?;
        self.conn
            .execute("DELETE FROM edges WHERE from_id=?1 OR to_id=?1", [id])?;
        Ok(())
    }

    pub fn nodes(&self) -> Result<Vec<NodeMeta>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, aliases, refs FROM nodes ORDER BY title COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(NodeMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                aliases: parse_json_vec(&r.get::<_, String>(2)?),
                refs: parse_json_vec(&r.get::<_, String>(3)?),
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Exact (trimmed, case-insensitive) match against a title or any alias.
    /// Deliberately returns at most one result and never a candidate list — recall,
    /// not recognition.
    pub fn find_by_title_or_alias(&self, attempt: &str) -> Result<Option<NodeMeta>> {
        let needle = attempt.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(None);
        }
        for node in self.nodes()? {
            if node.title.trim().to_lowercase() == needle
                || node
                    .aliases
                    .iter()
                    .any(|a| a.trim().to_lowercase() == needle)
            {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    pub fn backlinks(&self, id: &str) -> Result<Vec<Backlink>> {
        let now = Utc::now();
        let mut stmt = self.conn.prepare(
            "SELECT e.from_id, n.title, e.justification, e.recall_strength, e.last_recalled
             FROM edges e JOIN nodes n ON n.id = e.from_id
             WHERE e.to_id = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |r| {
            let last_recalled: Option<String> = r.get(4)?;
            Ok(Backlink {
                from_id: r.get(0)?,
                from_title: r.get(1)?,
                why: r.get(2)?,
                recall_strength: r.get(3)?,
                decay: decay(last_recalled.as_deref(), now),
                last_recalled,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn outgoing(&self, id: &str) -> Result<Vec<OutLink>> {
        let now = Utc::now();
        let mut stmt = self.conn.prepare(
            "SELECT e.to_id, n.title, e.justification, e.last_recalled
             FROM edges e JOIN nodes n ON n.id = e.to_id
             WHERE e.from_id = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |r| {
            let last_recalled: Option<String> = r.get(3)?;
            Ok(OutLink {
                to_id: r.get(0)?,
                to_title: r.get(1)?,
                why: r.get(2)?,
                decay: decay(last_recalled.as_deref(), now),
                last_recalled,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Record one recall rep for an edge: success strengthens it, failure weakens it.
    /// Both outcomes are logged — failures are the highest-signal "what to review" data.
    pub fn record_recall(&self, from_id: &str, to_id: &str, success: bool) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let delta = if success { 1.0 } else { -1.0 };
        self.conn.execute(
            "UPDATE edges
             SET last_recalled = ?1,
                 recall_strength = MAX(0, recall_strength + ?2)
             WHERE from_id = ?3 AND to_id = ?4",
            rusqlite::params![now, delta, from_id, to_id],
        )?;
        self.conn.execute(
            "INSERT INTO recall_log (from_id, to_id, success, ts) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![from_id, to_id, success as i32, now],
        )?;
        Ok(())
    }

    /// Restore a decayed edge: stamp it exercised now (resetting decay to 0) and
    /// strengthen it. The re-justification *friction* is enforced upstream by
    /// `commit_edge` (which rejects an empty reason); this only touches the recall
    /// telemetry. Errors if the edge does not exist. Not logged to `recall_log` —
    /// re-justifying is not a recall quiz, so it must not pollute the review stats.
    pub fn restore_edge(&self, from_id: &str, to_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let n = self.conn.execute(
            "UPDATE edges
             SET last_recalled = ?1, recall_strength = recall_strength + 1
             WHERE from_id = ?2 AND to_id = ?3",
            rusqlite::params![now, from_id, to_id],
        )?;
        if n == 0 {
            return Err(anyhow::anyhow!("no such edge to restore"));
        }
        Ok(())
    }

    /// Edges whose spaced-repetition schedule says they are due for review now, most
    /// overdue first. The justification is deliberately omitted — see [`DueReview`].
    pub fn due_reviews(&self) -> Result<Vec<DueReview>> {
        let now = Utc::now();
        let mut stmt = self.conn.prepare(
            "SELECT e.from_id, f.title, e.to_id, t.title, e.recall_strength, e.last_recalled
             FROM edges e
             JOIN nodes f ON f.id = e.from_id
             JOIN nodes t ON t.id = e.to_id",
        )?;
        let mut due: Vec<(DueReview, f64)> = stmt
            .query_map([], |r| {
                let recall_strength: f64 = r.get(4)?;
                let last_recalled: Option<String> = r.get(5)?;
                let ratio = review_overdue_ratio(recall_strength, last_recalled.as_deref(), now);
                Ok((
                    DueReview {
                        from_id: r.get(0)?,
                        from_title: r.get(1)?,
                        to_id: r.get(2)?,
                        to_title: r.get(3)?,
                        recall_strength,
                        last_recalled,
                    },
                    ratio,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter(|(_, ratio)| *ratio >= 1.0)
            .collect();
        // Most overdue first.
        due.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(due.into_iter().map(|(d, _)| d).collect())
    }

    /// The stored justification and current recall strength for a single edge, if it
    /// exists. Used to reveal + reschedule one connection during a review.
    pub fn edge(&self, from_id: &str, to_id: &str) -> Result<Option<(String, f64)>> {
        let row = self
            .conn
            .query_row(
                "SELECT justification, recall_strength FROM edges WHERE from_id=?1 AND to_id=?2",
                rusqlite::params![from_id, to_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    /// The connections most often failed in recall, drawn from the full `recall_log`
    /// history, most-failed first. Only connections whose endpoints both still exist
    /// and that have failed at least once appear. The justification is withheld.
    pub fn most_failed_connections(&self, limit: i64) -> Result<Vec<FailedConnection>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.from_id, f.title, l.to_id, t.title,
                    SUM(1 - l.success) AS failures,
                    COUNT(*) AS attempts
             FROM recall_log l
             JOIN nodes f ON f.id = l.from_id
             JOIN nodes t ON t.id = l.to_id
             GROUP BY l.from_id, l.to_id
             HAVING failures > 0
             ORDER BY failures DESC, attempts DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(FailedConnection {
                from_id: r.get(0)?,
                from_title: r.get(1)?,
                to_id: r.get(2)?,
                to_title: r.get(3)?,
                failures: r.get(4)?,
                attempts: r.get(5)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Full-text-ish search over title, aliases and body. The deliberate escape hatch.
    pub fn search(&self, query: &str) -> Result<Vec<NodeMeta>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{q}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, title, aliases, refs FROM nodes
             WHERE title LIKE ?1 OR aliases LIKE ?1 OR body LIKE ?1
             ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([&like], |r| {
            Ok(NodeMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                aliases: parse_json_vec(&r.get::<_, String>(2)?),
                refs: parse_json_vec(&r.get::<_, String>(3)?),
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Every distinct tag in the vault with its note count, most-used first (ties
    /// broken alphabetically). The tag-browsing escape hatch (#18c): read-only, like
    /// search — it surveys what exists, it never creates a note or an edge.
    pub fn tags(&self) -> Result<Vec<TagCount>> {
        let mut stmt = self.conn.prepare("SELECT tags FROM nodes")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for tags_json in rows.filter_map(|r| r.ok()) {
            for tag in parse_json_vec(&tags_json) {
                let tag = tag.trim().to_string();
                if !tag.is_empty() {
                    *counts.entry(tag).or_default() += 1;
                }
            }
        }
        let mut out: Vec<TagCount> = counts
            .into_iter()
            .map(|(tag, count)| TagCount { tag, count })
            .collect();
        out.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()))
        });
        Ok(out)
    }

    /// The notes carrying a given tag (exact, trimmed, case-insensitive), title-sorted.
    /// Read-only navigation: it lists existing notes, never creating one. Matching is
    /// exact so `ml` never bleeds into `html`.
    pub fn notes_with_tag(&self, tag: &str) -> Result<Vec<NodeMeta>> {
        let needle = tag.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, title, aliases, refs, tags FROM nodes ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                NodeMeta {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    aliases: parse_json_vec(&r.get::<_, String>(2)?),
                    refs: parse_json_vec(&r.get::<_, String>(3)?),
                },
                parse_json_vec(&r.get::<_, String>(4)?),
            ))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter(|(_, tags)| tags.iter().any(|t| t.trim().to_lowercase() == needle))
            .map(|(node, _)| node)
            .collect())
    }
}

fn parse_json_vec(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Link, Vault};
    use tempfile::TempDir;

    /// Build a vault with A -> B (justified) and return (dir, vault, ids).
    fn fixture() -> (TempDir, Vault, String, String) {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let b = v.create_note("Backprop", vec![], vec!["Backpropagation".into()], vec![], "").unwrap();
        let mut a = v.create_note("Transformers", vec![], vec![], vec![], "").unwrap();
        a.links.push(Link { to: b.id.clone(), why: "trained with gradient descent via backprop".into() });
        v.write_note(&a).unwrap();
        (dir, v, a.id, b.id)
    }

    #[test]
    fn backlinks_and_outgoing_reflect_the_graph() {
        let (_d, v, a_id, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        let back = idx.backlinks(&b_id).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].from_id, a_id);
        assert_eq!(back[0].from_title, "Transformers");
        assert!(back[0].why.contains("backprop"));

        let out = idx.outgoing(&a_id).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to_id, b_id);
    }

    #[test]
    fn rebuild_is_idempotent() {
        let (_d, v, _a, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap().len(), 1);
        assert_eq!(idx.nodes().unwrap().len(), 2);
    }

    #[test]
    fn record_recall_updates_strength_and_survives_rebuild() {
        let (_d, v, a_id, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        idx.record_recall(&a_id, &b_id, true).unwrap();
        idx.record_recall(&a_id, &b_id, true).unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap()[0].recall_strength, 2.0);

        // A subsequent sync must NOT wipe recall telemetry for surviving edges.
        idx.rebuild_from_vault(&v).unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap()[0].recall_strength, 2.0);

        idx.record_recall(&a_id, &b_id, false).unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap()[0].recall_strength, 1.0);
    }

    #[test]
    fn find_by_title_or_alias_is_exact_not_fuzzy() {
        let (_d, v, _a, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        assert_eq!(idx.find_by_title_or_alias("  backprop ").unwrap().unwrap().id, b_id);
        assert_eq!(idx.find_by_title_or_alias("Backpropagation").unwrap().unwrap().id, b_id);
        assert!(idx.find_by_title_or_alias("backpro").unwrap().is_none(), "no fuzzy match");
    }

    #[test]
    fn decay_grows_from_fresh_to_fully_faded() {
        let now = DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // No exercise timestamp at all → fully faded.
        assert_eq!(decay(None, now), 1.0);
        // Just exercised → fresh.
        assert!(decay(Some(&now.to_rfc3339()), now) < 0.001);
        // Halfway through the horizon → ~0.5.
        let half = (now - chrono::Duration::days(15)).to_rfc3339();
        assert!((decay(Some(&half), now) - 0.5).abs() < 0.02);
        // Past the horizon → clamped at 1.0.
        let stale = (now - chrono::Duration::days(60)).to_rfc3339();
        assert_eq!(decay(Some(&stale), now), 1.0);
    }

    #[test]
    fn a_just_justified_edge_is_fresh() {
        let (_d, v, _a, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        // Justifying a link counts as exercising it: a brand-new edge is not faded.
        assert!(idx.backlinks(&b_id).unwrap()[0].decay < 0.001);
    }

    #[test]
    fn restore_resets_decay_and_strengthens() {
        let (_d, v, a_id, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        // Age the edge well past the horizon so it is fully faded.
        let old = (Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        idx.conn
            .execute("UPDATE edges SET last_recalled = ?1", [&old])
            .unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap()[0].decay, 1.0);

        // Re-justifying restores it: fresh again and one rep stronger.
        idx.restore_edge(&a_id, &b_id).unwrap();
        let bl = idx.backlinks(&b_id).unwrap();
        assert!(bl[0].decay < 0.001, "restore makes the edge fresh again");
        assert_eq!(bl[0].recall_strength, 1.0, "restore strengthens the edge");

        // Restoring a non-existent edge is an error, not a silent no-op.
        assert!(idx.restore_edge(&a_id, "no-such-id").is_err());
    }

    #[test]
    fn review_interval_grows_with_strength() {
        assert_eq!(review_interval_days(0.0), 1.0);
        assert_eq!(review_interval_days(1.0), 2.0);
        assert_eq!(review_interval_days(3.0), 8.0);
    }

    #[test]
    fn is_due_respects_the_schedule() {
        let now = DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // Never exercised → always due.
        assert!(is_due(5.0, None, now));
        // Strength 0 (1-day interval): not due at 12h, due at 2 days.
        let twelve_h = (now - chrono::Duration::hours(12)).to_rfc3339();
        assert!(!is_due(0.0, Some(&twelve_h), now));
        let two_days = (now - chrono::Duration::days(2)).to_rfc3339();
        assert!(is_due(0.0, Some(&two_days), now));
        // Strength 3 (8-day interval): not due at 5 days, due at 9.
        let five = (now - chrono::Duration::days(5)).to_rfc3339();
        assert!(!is_due(3.0, Some(&five), now));
        let nine = (now - chrono::Duration::days(9)).to_rfc3339();
        assert!(is_due(3.0, Some(&nine), now));
    }

    #[test]
    fn due_reviews_lists_overdue_edges_most_overdue_first() {
        // Two edges into the same target: A->B (strength 0) and C->B (strength 3).
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let b = v.create_note("B", vec![], vec![], vec![], "").unwrap();
        let mut a = v.create_note("A", vec![], vec![], vec![], "").unwrap();
        let mut c = v.create_note("C", vec![], vec![], vec![], "").unwrap();
        a.links.push(Link { to: b.id.clone(), why: "a why".into() });
        c.links.push(Link { to: b.id.clone(), why: "c why".into() });
        v.write_note(&a).unwrap();
        v.write_note(&c).unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        // Freshly created edges (last_recalled = now) are not yet due.
        assert!(idx.due_reviews().unwrap().is_empty());

        // A: 3 days stale, strength 0 (interval 1d) → 3x overdue.
        // C: 10 days stale, strength 3 (interval 8d) → 1.25x overdue.
        let a_old = (Utc::now() - chrono::Duration::days(3)).to_rfc3339();
        let c_old = (Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        idx.conn
            .execute("UPDATE edges SET last_recalled=?1, recall_strength=0 WHERE from_id=?2", rusqlite::params![a_old, a.id])
            .unwrap();
        idx.conn
            .execute("UPDATE edges SET last_recalled=?1, recall_strength=3 WHERE from_id=?2", rusqlite::params![c_old, c.id])
            .unwrap();

        let due = idx.due_reviews().unwrap();
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].from_id, a.id, "more-overdue A ranks first");
        assert_eq!(due[1].from_id, c.id);
        // Justification is withheld from the queue.
        assert_eq!(due[0].to_title, "B");
    }

    #[test]
    fn what_to_review_ranks_most_failed_connections() {
        // Two edges into B: A->B and C->B.
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let b = v.create_note("B", vec![], vec![], vec![], "").unwrap();
        let mut a = v.create_note("A", vec![], vec![], vec![], "").unwrap();
        let mut c = v.create_note("C", vec![], vec![], vec![], "").unwrap();
        a.links.push(Link { to: b.id.clone(), why: "a why".into() });
        c.links.push(Link { to: b.id.clone(), why: "c why".into() });
        v.write_note(&a).unwrap();
        v.write_note(&c).unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        // A->B: 2 failures / 3 attempts. C->B: 1 failure / 2 attempts.
        idx.record_recall(&a.id, &b.id, false).unwrap();
        idx.record_recall(&a.id, &b.id, false).unwrap();
        idx.record_recall(&a.id, &b.id, true).unwrap();
        idx.record_recall(&c.id, &b.id, false).unwrap();
        idx.record_recall(&c.id, &b.id, true).unwrap();

        let review = idx.most_failed_connections(10).unwrap();
        assert_eq!(review.len(), 2);
        assert_eq!(review[0].from_id, a.id, "most-failed connection ranks first");
        assert_eq!(review[0].from_title, "A");
        assert_eq!(review[0].to_title, "B");
        assert_eq!(review[0].failures, 2);
        assert_eq!(review[0].attempts, 3);
        assert_eq!(review[1].from_id, c.id);
        assert_eq!(review[1].failures, 1);

        // A connection that was always recalled never surfaces; the limit is honored.
        idx.record_recall(&a.id, &b.id, true).unwrap();
        assert_eq!(idx.most_failed_connections(1).unwrap().len(), 1);
    }

    #[test]
    fn search_matches_body_and_title() {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        v.create_note("Adam", vec![], vec![], vec![], "adaptive moment estimation").unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        assert_eq!(idx.search("moment").unwrap().len(), 1);
        assert_eq!(idx.search("Adam").unwrap().len(), 1);
        assert_eq!(idx.search("nonexistent").unwrap().len(), 0);
    }

    #[test]
    fn tags_counts_and_notes_by_tag_filters_exactly() {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        v.create_note("A", vec![], vec![], vec!["ml".into(), "nlp".into()], "").unwrap();
        v.create_note("B", vec![], vec![], vec!["ml".into()], "").unwrap();
        v.create_note("C", vec![], vec![], vec!["html".into()], "").unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        // Counted, most-used first; ties broken alphabetically (html before nlp).
        let tags = idx.tags().unwrap();
        assert_eq!(tags[0], TagCount { tag: "ml".into(), count: 2 });
        assert_eq!(
            tags.iter().map(|t| t.tag.as_str()).collect::<Vec<_>>(),
            vec!["ml", "html", "nlp"],
        );

        // Exact match: "ml" must NOT bleed into "html".
        let ml = idx.notes_with_tag("ml").unwrap();
        assert_eq!(ml.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        assert_eq!(idx.notes_with_tag("  ML ").unwrap().len(), 2, "trimmed + case-insensitive");
        assert_eq!(idx.notes_with_tag("html").unwrap().len(), 1);
        assert!(idx.notes_with_tag("missing").unwrap().is_empty());
        assert!(idx.notes_with_tag("  ").unwrap().is_empty());
    }

    #[test]
    fn tags_repopulate_after_a_rebuild() {
        let (_d, v, a_id, _b) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert!(idx.tags().unwrap().is_empty(), "fixture notes start untagged");

        // Tag a note, re-sync: the index reflects the new tag.
        let mut a = v.read_note(&a_id).unwrap();
        a.tags = vec!["seminal".into()];
        v.write_note(&a).unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert_eq!(idx.tags().unwrap(), vec![TagCount { tag: "seminal".into(), count: 1 }]);
        assert_eq!(idx.notes_with_tag("seminal").unwrap()[0].id, a_id);
    }

    #[test]
    fn migrate_backfills_tags_column_on_pre_18c_indexes() {
        // An index created before #18c has a nodes table without the tags column.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (
                id TEXT PRIMARY KEY, title TEXT NOT NULL,
                aliases TEXT NOT NULL DEFAULT '[]', refs TEXT NOT NULL DEFAULT '[]',
                body TEXT NOT NULL DEFAULT ''
            );",
        )
        .unwrap();
        let idx = Index { conn };
        assert!(!idx.column_exists("nodes", "tags").unwrap());

        idx.migrate().unwrap(); // adds the column
        assert!(idx.column_exists("nodes", "tags").unwrap());
        idx.migrate().unwrap(); // idempotent: a second migrate is a no-op
        assert!(idx.tags().unwrap().is_empty());
    }
}
