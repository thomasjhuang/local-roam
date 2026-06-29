//! Index — the SQLite mirror of the vault.
//!
//! The vault (Markdown files) is always the source of truth; this index is a cache
//! for fast queries (backlinks, search) and the home of recall *telemetry*
//! (`last_recalled`, `recall_strength`). Knowledge lives in the files; if this index
//! is lost it can be rebuilt with [`Index::rebuild_from_vault`] — only usage stats,
//! not knowledge, are lost.

use crate::vault::{Note, Vault};
use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

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
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct OutLink {
    pub to_id: String,
    pub to_title: String,
    pub why: String,
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
        Ok(())
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
            "INSERT INTO nodes (id, title, aliases, refs, body)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, aliases=excluded.aliases,
                refs=excluded.refs, body=excluded.body",
            rusqlite::params![
                note.id,
                note.title,
                serde_json::to_string(&note.aliases)?,
                serde_json::to_string(&note.refs)?,
                note.body,
            ],
        )?;

        // Upsert edges; keep recall stats on conflict (only justification changes).
        let keep_targets: Vec<String> = note.links.iter().map(|l| l.to.clone()).collect();
        for link in &note.links {
            self.conn.execute(
                "INSERT INTO edges (from_id, to_id, justification)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(from_id, to_id) DO UPDATE SET justification=excluded.justification",
                rusqlite::params![note.id, link.to, link.why],
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
        let mut stmt = self.conn.prepare(
            "SELECT e.from_id, n.title, e.justification, e.recall_strength
             FROM edges e JOIN nodes n ON n.id = e.from_id
             WHERE e.to_id = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(Backlink {
                from_id: r.get(0)?,
                from_title: r.get(1)?,
                why: r.get(2)?,
                recall_strength: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn outgoing(&self, id: &str) -> Result<Vec<OutLink>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.to_id, n.title, e.justification
             FROM edges e JOIN nodes n ON n.id = e.to_id
             WHERE e.from_id = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(OutLink {
                to_id: r.get(0)?,
                to_title: r.get(1)?,
                why: r.get(2)?,
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
}
