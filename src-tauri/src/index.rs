//! Index — the SQLite mirror of the vault.
//!
//! The vault (Markdown files) is always the source of truth; this index is a cache
//! for fast queries (backlinks, search). Knowledge lives in the files; if this index
//! is lost it can be rebuilt with [`Index::rebuild_from_vault`] — nothing of value is
//! lost, only the cache.

use crate::folgezettel;
use crate::vault::{self, Entry, Note, Thread, Vault};
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

// --- v3 card/thread read models (#22) ----------------------------------------------

/// A card row: its opaque id, its first-line label, and its optional title.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct CardMeta {
    pub id: String,
    pub label: String,
    pub title: Option<String>,
}

/// A thread row: its id, title, refs (a paper thread carries them), and card count.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ThreadMeta {
    pub id: String,
    pub title: String,
    pub refs: Vec<String>,
    pub card_count: i64,
}

/// A card as it sits in one thread: its derived address, label, and manifest position.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ThreadCard {
    pub card_id: String,
    pub address: String,
    pub label: String,
    pub position: i64,
}

/// One of a card's memberships: a thread it belongs to and its derived address there.
/// A card in two threads yields two of these — two addresses for one card.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct CardMembership {
    pub thread_id: String,
    pub thread_title: String,
    pub address: String,
}

pub struct Index {
    conn: Connection,
}

impl Index {
    /// An ephemeral in-memory index — a test helper only; the app always opens on disk.
    #[cfg(test)]
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
            );
            -- v3 card/thread model (#22). All four tables are a fully-derived cache:
            -- rebuild_from_vault clears and repopulates them from the files, so deleting
            -- the index loses nothing. Folgezettel addresses are derived, never stored
            -- as truth — the `address` column is recomputed on every rebuild.
            CREATE TABLE IF NOT EXISTS cards (
                id      TEXT PRIMARY KEY,
                title   TEXT,
                body    TEXT NOT NULL DEFAULT '',
                tags    TEXT NOT NULL DEFAULT '[]',
                created TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS threads (
                id      TEXT PRIMARY KEY,
                title   TEXT NOT NULL,
                refs    TEXT NOT NULL DEFAULT '[]',
                tags    TEXT NOT NULL DEFAULT '[]',
                created TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS membership (
                thread_id TEXT NOT NULL,
                card_id   TEXT NOT NULL,
                address   TEXT NOT NULL,
                position  INTEGER NOT NULL,
                PRIMARY KEY (thread_id, card_id)
            );
            CREATE TABLE IF NOT EXISTS card_links (
                from_card TEXT NOT NULL,
                to_target TEXT NOT NULL,
                PRIMARY KEY (from_card, to_target)
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

    /// Sync the index to match the vault. Reads every classified file (cards, threads,
    /// legacy notes) and rebuilds both the legacy node/edge cache (for the current UI)
    /// and the v3 card/thread/membership model. Idempotent: running it twice on an
    /// unchanged vault yields the same index.
    pub fn rebuild_from_vault(&self, vault: &Vault) -> Result<()> {
        let entries = vault.list_entries()?;

        // --- legacy node/edge cache (the current, pre-#23 UI reads this) ---
        let notes: Vec<&Note> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Legacy(n) => Some(n),
                _ => None,
            })
            .collect();
        let keep_ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();
        for note in &notes {
            self.reindex_note(note)?;
        }
        // Drop nodes/edges that no longer exist on disk.
        let placeholders = std::iter::repeat_n("?", keep_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        if keep_ids.is_empty() {
            self.conn.execute("DELETE FROM nodes", [])?;
            self.conn.execute("DELETE FROM edges", [])?;
        } else {
            self.conn.execute(
                &format!("DELETE FROM nodes WHERE id NOT IN ({placeholders})"),
                rusqlite::params_from_iter(keep_ids.iter()),
            )?;
            self.conn.execute(
                &format!("DELETE FROM edges WHERE from_id NOT IN ({placeholders})"),
                rusqlite::params_from_iter(keep_ids.iter()),
            )?;
        }

        // --- v3 card/thread model (fully derived: clear and repopulate) ---
        self.rebuild_cards_and_threads(&entries)?;
        Ok(())
    }

    /// Rebuild the card/thread/membership/link tables from every classified entry.
    /// A legacy note migrates to a single-card thread of the same title, its frontmatter
    /// `links` becoming card-body wiki-links `[[id]] — <why>` (the whys survive as
    /// prose). Paper/source notes (which carry refs) migrate to paper threads.
    fn rebuild_cards_and_threads(&self, entries: &[Entry]) -> Result<()> {
        self.conn.execute("DELETE FROM cards", [])?;
        self.conn.execute("DELETE FROM threads", [])?;
        self.conn.execute("DELETE FROM membership", [])?;
        self.conn.execute("DELETE FROM card_links", [])?;

        for entry in entries {
            match entry {
                Entry::Card(card) => {
                    self.insert_card(&card.id, card.title.as_deref(), &card.body, &card.tags, &card.created)?;
                }
                Entry::Thread(thread) => {
                    self.insert_thread(thread)?;
                }
                Entry::Legacy(note) => {
                    self.migrate_legacy(note)?;
                }
            }
        }
        Ok(())
    }

    fn insert_card(
        &self,
        id: &str,
        title: Option<&str>,
        body: &str,
        tags: &[String],
        created: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cards (id, title, body, tags, created) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, body=excluded.body,
                tags=excluded.tags, created=excluded.created",
            rusqlite::params![id, title, body, serde_json::to_string(tags)?, created],
        )?;
        for target in vault::wikilinks(body) {
            self.conn.execute(
                "INSERT OR IGNORE INTO card_links (from_card, to_target) VALUES (?1, ?2)",
                rusqlite::params![id, target],
            )?;
        }
        Ok(())
    }

    fn insert_thread(&self, thread: &Thread) -> Result<()> {
        self.conn.execute(
            "INSERT INTO threads (id, title, refs, tags, created) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, refs=excluded.refs,
                tags=excluded.tags, created=excluded.created",
            rusqlite::params![
                thread.id,
                thread.title,
                serde_json::to_string(&thread.refs)?,
                serde_json::to_string(&thread.tags)?,
                thread.created,
            ],
        )?;
        // Derive addresses from the manifest position — never stored on disk.
        for addr in folgezettel::addresses(&thread.manifest) {
            self.conn.execute(
                "INSERT OR REPLACE INTO membership (thread_id, card_id, address, position)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![thread.id, addr.card_id, addr.address, addr.position as i64],
            )?;
        }
        Ok(())
    }

    /// Migrate one legacy note to a single-card thread + card.
    fn migrate_legacy(&self, note: &Note) -> Result<()> {
        let card_id = format!("{}-card", note.id);
        let body = migrated_card_body(note);
        // The card holds the note's atom + its links-as-prose; it is untitled (the
        // thread carries the title).
        self.insert_card(&card_id, None, &body, &[], &note.created)?;

        let thread = Thread {
            id: note.id.clone(),
            title: note.title.clone(),
            created: note.created.clone(),
            tags: note.tags.clone(),
            refs: note.refs.clone(),
            manifest: vec![folgezettel::ManifestNode::leaf(card_id)],
        };
        self.insert_thread(&thread)?;
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

    // --- v3 card/thread queries (#22) ----------------------------------------------

    /// Every card, with its first-line label.
    pub fn cards(&self) -> Result<Vec<CardMeta>> {
        let mut stmt = self.conn.prepare("SELECT id, title, body FROM cards")?;
        let rows = stmt.query_map([], |r| {
            let body: String = r.get(2)?;
            Ok(CardMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                label: label(&body),
            })
        })?;
        let mut out: Vec<CardMeta> = rows.filter_map(|r| r.ok()).collect();
        out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        Ok(out)
    }

    /// Every thread with its card count, title-sorted.
    pub fn threads(&self) -> Result<Vec<ThreadMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, t.refs,
                    (SELECT COUNT(*) FROM membership m WHERE m.thread_id = t.id) AS card_count
             FROM threads t
             ORDER BY t.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ThreadMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                refs: parse_json_vec(&r.get::<_, String>(2)?),
                card_count: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// The cards of one thread in manifest (address) order, each with its derived
    /// Folgezettel address and first-line label.
    pub fn thread_cards(&self, thread_id: &str) -> Result<Vec<ThreadCard>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.card_id, m.address, m.position, c.body
             FROM membership m
             LEFT JOIN cards c ON c.id = m.card_id
             WHERE m.thread_id = ?1
             ORDER BY m.position",
        )?;
        let rows = stmt.query_map([thread_id], |r| {
            let body: Option<String> = r.get(3)?;
            Ok(ThreadCard {
                card_id: r.get(0)?,
                address: r.get(1)?,
                position: r.get(2)?,
                label: label(&body.unwrap_or_default()),
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Every thread a card belongs to, with its derived address there. A card in two
    /// threads returns two rows — two addresses for the one card.
    pub fn card_memberships(&self, card_id: &str) -> Result<Vec<CardMembership>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.thread_id, t.title, m.address
             FROM membership m
             JOIN threads t ON t.id = m.thread_id
             WHERE m.card_id = ?1
             ORDER BY t.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([card_id], |r| {
            Ok(CardMembership {
                thread_id: r.get(0)?,
                thread_title: r.get(1)?,
                address: r.get(2)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// The ids a card links to (parsed from its body wiki-links). A target may be a
    /// card or a thread; resolution is left to the caller.
    pub fn card_targets(&self, card_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT to_target FROM card_links WHERE from_card = ?1 ORDER BY to_target")?;
        let rows = stmt.query_map([card_id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

fn parse_json_vec(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

/// The migrated body for a legacy note's single card: the note's own body followed by
/// its frontmatter links rendered as wiki-link prose, so the whys survive as text.
fn migrated_card_body(note: &Note) -> String {
    let mut body = note.body.trim_end().to_string();
    for link in &note.links {
        if !body.is_empty() {
            body.push('\n');
        }
        if link.why.trim().is_empty() {
            body.push_str(&format!("[[{}]]", link.to));
        } else {
            body.push_str(&format!("[[{}]] — {}", link.to, link.why.trim()));
        }
    }
    body
}

/// A card's display label: its first non-empty line (the v3 model gives cards no
/// required title, so the first line is the label everywhere).
fn label(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
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

    // --- v3 card/thread model (#22) ------------------------------------------------

    use crate::folgezettel::ManifestNode;

    #[test]
    fn legacy_note_migrates_to_a_single_card_thread_preserving_whys() {
        // A legacy note A → B (with a why) plus its own body.
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let b = v.create_note("Backprop", vec![], vec![], vec![], "").unwrap();
        let mut a = v
            .create_note("Transformers", vec![], vec![], vec!["nlp".into()], "Self-attention over tokens.")
            .unwrap();
        a.links.push(Link { to: b.id.clone(), why: "trained via backprop".into() });
        v.write_note(&a).unwrap();

        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        // Each note became a thread of the same title.
        let threads = idx.threads().unwrap();
        let titles: Vec<&str> = threads.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Transformers") && titles.contains(&"Backprop"));

        // The Transformers thread has exactly one card, at address "1".
        let cards = idx.thread_cards(&a.id).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].address, "1");
        assert_eq!(cards[0].card_id, format!("{}-card", a.id));

        // The why survives as prose in the migrated card body, and the link is indexed.
        let card_body = idx
            .conn
            .query_row("SELECT body FROM cards WHERE id=?1", [&cards[0].card_id], |r| r.get::<_, String>(0))
            .unwrap();
        assert!(card_body.contains("Self-attention over tokens."), "the note body survives");
        assert!(card_body.contains(&format!("[[{}]] — trained via backprop", b.id)), "the why survives as prose");
        assert_eq!(idx.card_targets(&cards[0].card_id).unwrap(), vec![b.id.clone()]);
    }

    #[test]
    fn paper_note_with_refs_migrates_to_a_paper_thread() {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        v.create_note("Attention Is All You Need", vec!["arXiv:1706.03762".into()], vec![], vec![], "seminal")
            .unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        let paper = idx.threads().unwrap().into_iter().find(|t| t.title.contains("Attention")).unwrap();
        assert_eq!(paper.refs, vec!["arXiv:1706.03762".to_string()], "refs → paper thread manifest");
    }

    #[test]
    fn a_card_in_two_threads_has_two_addresses_in_the_index() {
        // Native v3 files: one card shared by two threads at different positions.
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let shared = v.create_card("the shared atom", vec![]).unwrap();
        let other = v.create_card("another atom", vec![]).unwrap();
        // Thread one: [other, shared] → shared is "2".
        let t1 = v
            .create_thread("One", vec![], vec![], vec![
                ManifestNode::leaf(&other.id),
                ManifestNode::leaf(&shared.id),
            ])
            .unwrap();
        // Thread two: [shared] → shared is "1".
        let t2 = v
            .create_thread("Two", vec![], vec![], vec![ManifestNode::leaf(&shared.id)])
            .unwrap();

        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        let mut memberships = idx.card_memberships(&shared.id).unwrap();
        memberships.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
        let addr = |tid: &str| memberships.iter().find(|m| m.thread_id == tid).unwrap().address.clone();
        assert_eq!(memberships.len(), 2, "one card, two threads, two memberships");
        assert_eq!(addr(&t1.id), "2");
        assert_eq!(addr(&t2.id), "1");
    }

    #[test]
    fn rebuild_of_the_new_model_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        // A mix: a legacy note (to migrate) and native card/thread files.
        v.create_note("Legacy", vec![], vec![], vec![], "old idea").unwrap();
        let c = v.create_card("native atom", vec![]).unwrap();
        v.create_thread("Native", vec![], vec![], vec![ManifestNode::leaf(&c.id)]).unwrap();

        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        let threads1 = idx.threads().unwrap();
        let cards1 = idx.cards().unwrap();

        idx.rebuild_from_vault(&v).unwrap();
        assert_eq!(idx.threads().unwrap(), threads1, "threads stable across rebuild");
        assert_eq!(idx.cards().unwrap(), cards1, "cards stable across rebuild");
    }

    #[test]
    fn deleting_and_rebuilding_the_index_loses_nothing() {
        // Everything the new model shows is derived from files — a fresh index rebuilt
        // from the same vault is identical.
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let c = v.create_card("atom one", vec!["t".into()]).unwrap();
        let thread = v.create_thread("A thread", vec!["doi:1".into()], vec![], vec![ManifestNode::leaf(&c.id)]).unwrap();

        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        let before = idx.thread_cards(&thread.id).unwrap();

        // "Delete" the index by opening a brand-new one and rebuilding from the files.
        let fresh = Index::open_in_memory().unwrap();
        fresh.rebuild_from_vault(&v).unwrap();
        assert_eq!(fresh.thread_cards(&thread.id).unwrap(), before);
        assert_eq!(fresh.threads().unwrap(), idx.threads().unwrap());
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
