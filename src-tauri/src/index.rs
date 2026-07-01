//! Index — the SQLite mirror of the vault.
//!
//! The vault (Markdown files) is always the source of truth; this index is a cache
//! for fast queries (backlinks, search). Knowledge lives in the files; if this index
//! is lost it can be rebuilt with [`Index::rebuild_from_vault`] — nothing of value is
//! lost, only the cache.

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
                from_id       TEXT NOT NULL,
                to_id         TEXT NOT NULL,
                justification TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (from_id, to_id)
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

    /// Sync the index to match the vault. Idempotent: running it twice on an unchanged
    /// vault is a no-op.
    pub fn rebuild_from_vault(&self, vault: &Vault) -> Result<()> {
        let notes = vault.list_notes()?;
        let keep_ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();

        for note in &notes {
            self.reindex_note(note)?;
        }

        // Drop nodes that no longer exist on disk.
        let placeholders = std::iter::repeat_n("?", keep_ids.len())
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

    /// Upsert a single note's node row and its outgoing edges.
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
            let ph = std::iter::repeat_n("?", keep_targets.len())
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
    /// Deliberately returns at most one result — a stopgap resolver until the
    /// card/thread model (#22/#23) replaces linking.
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
            "SELECT e.from_id, n.title, e.justification
             FROM edges e JOIN nodes n ON n.id = e.from_id
             WHERE e.to_id = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(Backlink {
                from_id: r.get(0)?,
                from_title: r.get(1)?,
                why: r.get(2)?,
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
    fn find_by_title_or_alias_is_exact_not_fuzzy() {
        let (_d, v, _a, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        assert_eq!(idx.find_by_title_or_alias("  backprop ").unwrap().unwrap().id, b_id);
        assert_eq!(idx.find_by_title_or_alias("Backpropagation").unwrap().unwrap().id, b_id);
        assert!(idx.find_by_title_or_alias("backpro").unwrap().is_none(), "no fuzzy match");
    }

    #[test]
    fn removing_a_link_drops_the_edge_on_rebuild() {
        let (_d, v, a_id, b_id) = fixture();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert_eq!(idx.backlinks(&b_id).unwrap().len(), 1);

        // Drop A's link and re-sync: the edge disappears.
        let mut a = v.read_note(&a_id).unwrap();
        a.links.clear();
        v.write_note(&a).unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert!(idx.backlinks(&b_id).unwrap().is_empty());
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
