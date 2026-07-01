//! Vault — owns the on-disk Markdown files and frontmatter (de)serialization.
//!
//! This is the ONLY module that knows the file format. Every note is one Markdown
//! file named `<id>.md` with a YAML frontmatter block. The `id` is the source of
//! truth and never changes, so titles can be renamed without breaking links.
//!
//! Edges are authored from the *source* note and stored in its frontmatter `links`
//! list (each with a mandatory `why` justification). This keeps the vault the single
//! source of truth — the SQLite Index is always rebuildable from these files.

use crate::folgezettel::ManifestNode;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A justified, directional link from one note to another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    /// id of the target note.
    pub to: String,
    /// One-sentence justification for the connection. Never empty.
    pub why: String,
}

/// A note: a paper or an idea. A paper is just a note that carries a `ref`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub created: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub body: String,
}

/// Frontmatter is everything in a note except the Markdown body.
#[derive(Serialize, Deserialize)]
struct Frontmatter {
    id: String,
    title: String,
    created: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    refs: Vec<String>,
    #[serde(default)]
    links: Vec<Link>,
}

// --- v3 card/thread model (#22) ----------------------------------------------------
// Two kinds of note, discriminated by a `kind:` frontmatter field. A file with no
// `kind` (or an unknown one) is a legacy v1/v2 `Note`, migrated on index rebuild.

/// A CARD: one file, one atomic idea. The filename/id is an opaque stable id; the card
/// needs no title (its first body line is its label everywhere). Frontmatter is minimal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    /// Optional — cards are untitled by default; the first body line is the label.
    pub title: Option<String>,
    pub created: String,
    pub tags: Vec<String>,
    pub body: String,
}

/// A THREAD manifest: a titled structure note whose body is a nested Markdown list of
/// `[[card]]` links defining an ordered tree. Optional `refs` carry paper metadata (a
/// paper is a thread). No addresses are stored — they derive from `manifest` position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub created: String,
    pub tags: Vec<String>,
    pub refs: Vec<String>,
    pub manifest: Vec<ManifestNode>,
}

/// A classified vault file.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Card(Card),
    Thread(Thread),
    /// A legacy v1/v2 note, to be migrated into the card/thread model on rebuild.
    Legacy(Note),
}

#[derive(Serialize, Deserialize)]
struct CardFrontmatter {
    kind: String,
    id: String,
    created: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ThreadFrontmatter {
    kind: String,
    id: String,
    title: String,
    created: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    refs: Vec<String>,
}

/// Just enough of the frontmatter to route a file to the right parser.
#[derive(Deserialize)]
struct KindPeek {
    #[serde(default)]
    kind: Option<String>,
}

/// Classify raw file content into a card, a thread, or a legacy note.
pub fn classify(content: &str) -> Result<Entry> {
    let (yaml, body) = split_frontmatter(content)?;
    let peek: KindPeek = serde_yaml::from_str(yaml).unwrap_or(KindPeek { kind: None });
    match peek.kind.as_deref() {
        Some("card") => Ok(Entry::Card(parse_card(yaml, body)?)),
        Some("thread") => Ok(Entry::Thread(parse_thread(yaml, body)?)),
        _ => Ok(Entry::Legacy(parse_note(content)?)),
    }
}

fn parse_card(yaml: &str, body: &str) -> Result<Card> {
    let fm: CardFrontmatter = serde_yaml::from_str(yaml).context("invalid card frontmatter")?;
    if fm.id.trim().is_empty() {
        return Err(anyhow!("card has empty id"));
    }
    Ok(Card {
        id: fm.id,
        title: fm.title.filter(|t| !t.trim().is_empty()),
        created: fm.created,
        tags: fm.tags,
        body: body.to_string(),
    })
}

fn parse_thread(yaml: &str, body: &str) -> Result<Thread> {
    let fm: ThreadFrontmatter = serde_yaml::from_str(yaml).context("invalid thread frontmatter")?;
    if fm.id.trim().is_empty() {
        return Err(anyhow!("thread has empty id"));
    }
    Ok(Thread {
        id: fm.id,
        title: fm.title,
        created: fm.created,
        tags: fm.tags,
        refs: fm.refs,
        manifest: parse_manifest(body),
    })
}

/// Serialize a card back to file content. Part of the v3 write API wired up in #23.
#[allow(dead_code)]
pub fn serialize_card(card: &Card) -> Result<String> {
    let fm = CardFrontmatter {
        kind: "card".into(),
        id: card.id.clone(),
        created: card.created.clone(),
        tags: card.tags.clone(),
        title: card.title.clone(),
    };
    let yaml = serde_yaml::to_string(&fm)?;
    let body = card.body.trim_end();
    Ok(format!("---\n{yaml}---\n\n{body}\n"))
}

/// Serialize a thread (frontmatter + rendered manifest list) back to file content.
/// Part of the v3 write API wired up in #23.
#[allow(dead_code)]
pub fn serialize_thread(thread: &Thread) -> Result<String> {
    let fm = ThreadFrontmatter {
        kind: "thread".into(),
        id: thread.id.clone(),
        title: thread.title.clone(),
        created: thread.created.clone(),
        tags: thread.tags.clone(),
        refs: thread.refs.clone(),
    };
    let yaml = serde_yaml::to_string(&fm)?;
    let mut body = String::new();
    render_manifest(&thread.manifest, 0, &mut body);
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

/// Render a manifest tree as a nested Markdown bullet list (two spaces per level).
#[allow(dead_code)]
fn render_manifest(nodes: &[ManifestNode], depth: usize, out: &mut String) {
    for node in nodes {
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str("- [[");
        out.push_str(&node.card_id);
        out.push_str("]]\n");
        render_manifest(&node.children, depth + 1, out);
    }
}

/// Parse a nested Markdown list of `[[card]]` links into a manifest tree. Indentation
/// (any consistent unit) sets nesting; lines without a wiki-link are ignored, so blank
/// lines and stray prose don't break a hand-edited manifest.
fn parse_manifest(body: &str) -> Vec<ManifestNode> {
    // Flatten to (depth, card_id) using a stack of indent widths, so any indent unit
    // (2 spaces, 4 spaces, tabs) yields the same tree as long as it is consistent.
    let mut flat: Vec<(usize, String)> = Vec::new();
    let mut widths: Vec<usize> = Vec::new();
    for line in body.lines() {
        let Some(card_id) = extract_wikilink(line) else {
            continue;
        };
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        while widths.last().is_some_and(|w| indent < *w) {
            widths.pop();
        }
        if widths.last() != Some(&indent) {
            widths.push(indent);
        }
        flat.push((widths.len() - 1, card_id));
    }
    let mut pos = 0;
    build_level(&flat, &mut pos, 0)
}

fn build_level(items: &[(usize, String)], pos: &mut usize, depth: usize) -> Vec<ManifestNode> {
    let mut nodes: Vec<ManifestNode> = Vec::new();
    while let Some((d, id)) = items.get(*pos) {
        if *d < depth {
            break;
        }
        // A deeper-than-expected first item can only arise from malformed indentation;
        // treat it as a child of the previous node rather than dropping it.
        if *d > depth {
            if let Some(last) = nodes.last_mut() {
                last.children.append(&mut build_level(items, pos, depth + 1));
                continue;
            }
            break;
        }
        let id = id.clone();
        *pos += 1;
        let children = build_level(items, pos, depth + 1);
        nodes.push(ManifestNode { card_id: id, children });
    }
    nodes
}

/// The id inside the first `[[...]]` on a line, trimmed, or None.
fn extract_wikilink(line: &str) -> Option<String> {
    let start = line.find("[[")? + 2;
    let end = line[start..].find("]]")? + start;
    let id = line[start..end].trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// Every `[[target]]` reference in a block of text, in order (deduplicated). The card
/// body is where a card's outgoing links live in the v3 model, so the index parses
/// them out with this.
pub fn wikilinks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else { break };
        let id = after[..close].trim();
        if !id.is_empty() && !out.contains(&id.to_string()) {
            out.push(id.to_string());
        }
        rest = &after[close + 2..];
    }
    out
}

/// Split raw file content into (yaml frontmatter, body). Returns an error if the
/// file is not fronted by a `---` delimited block.
fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content); // tolerate BOM
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
        .ok_or_else(|| anyhow!("missing opening frontmatter delimiter"))?;
    // Find a line that is exactly `---`.
    let mut idx = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let yaml = &rest[..idx];
            let body = &rest[idx + line.len()..];
            return Ok((yaml, body.trim_start_matches(['\r', '\n'])));
        }
        idx += line.len();
    }
    Err(anyhow!("missing closing frontmatter delimiter"))
}

/// Parse raw file content into a Note.
pub fn parse_note(content: &str) -> Result<Note> {
    let (yaml, body) = split_frontmatter(content)?;
    let fm: Frontmatter = serde_yaml::from_str(yaml).context("invalid frontmatter yaml")?;
    if fm.id.trim().is_empty() {
        return Err(anyhow!("note has empty id"));
    }
    Ok(Note {
        id: fm.id,
        title: fm.title,
        created: fm.created,
        aliases: fm.aliases,
        tags: fm.tags,
        refs: fm.refs,
        links: fm.links,
        body: body.to_string(),
    })
}

/// Serialize a Note back to file content (frontmatter + body).
pub fn serialize_note(note: &Note) -> Result<String> {
    let fm = Frontmatter {
        id: note.id.clone(),
        title: note.title.clone(),
        created: note.created.clone(),
        aliases: note.aliases.clone(),
        tags: note.tags.clone(),
        refs: note.refs.clone(),
        links: note.links.clone(),
    };
    let yaml = serde_yaml::to_string(&fm)?;
    let body = note.body.trim_end();
    Ok(format!("---\n{yaml}---\n\n{body}\n"))
}

/// The vault: a folder of Markdown notes.
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open (or adopt) a folder as a vault, creating it if it does not exist.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)
            .with_context(|| format!("cannot create vault dir {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    /// Every classified file in the vault (cards, threads, legacy notes). Malformed
    /// files are skipped, never fatal.
    pub fn list_entries(&self) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for walked in WalkDir::new(&self.root)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = walked.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            match classify(&content) {
                Ok(entry) => entries.push(entry),
                Err(err) => eprintln!("skipping malformed file {}: {err:#}", path.display()),
            }
        }
        Ok(entries)
    }

    /// List every legacy note (a v1/v2 file with no card/thread `kind`). Card and
    /// thread files are skipped. Malformed files are skipped, never fatal.
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        Ok(self
            .list_entries()?
            .into_iter()
            .filter_map(|e| match e {
                Entry::Legacy(note) => Some(note),
                _ => None,
            })
            .collect())
    }

    pub fn read_note(&self, id: &str) -> Result<Note> {
        let path = self.path_for(id);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("note {id} not found"))?;
        parse_note(&content)
    }

    /// Upsert a note by id.
    pub fn write_note(&self, note: &Note) -> Result<()> {
        let content = serialize_note(note)?;
        std::fs::write(self.path_for(&note.id), content)
            .with_context(|| format!("cannot write note {}", note.id))?;
        Ok(())
    }

    /// Create a new note, generating its id and creation timestamp.
    pub fn create_note(
        &self,
        title: &str,
        refs: Vec<String>,
        aliases: Vec<String>,
        tags: Vec<String>,
        body: &str,
    ) -> Result<Note> {
        let note = Note {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            aliases,
            tags,
            refs,
            links: Vec::new(),
            body: body.to_string(),
        };
        self.write_note(&note)?;
        Ok(note)
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        let path = self.path_for(id);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("cannot delete note {id}"))?;
        }
        Ok(())
    }

    /// Rename a note's title, preserving its id (and therefore all links to it).
    /// Legacy API (#2); renames now flow through `save_note`, so it is currently unused.
    #[allow(dead_code)]
    pub fn rename_title(&self, id: &str, new_title: &str) -> Result<Note> {
        let mut note = self.read_note(id)?;
        note.title = new_title.trim().to_string();
        self.write_note(&note)?;
        Ok(note)
    }
}

/// The v3 card/thread write/read API. Landed with the data model (#22, "parse/write the
/// manifest tree"); its first non-test caller is the thread view / card editor (#23), so
/// it reads as dead code until then. Exercised now by the vault and index tests.
#[allow(dead_code)]
impl Vault {
    /// List every card file.
    pub fn list_cards(&self) -> Result<Vec<Card>> {
        Ok(self
            .list_entries()?
            .into_iter()
            .filter_map(|e| match e {
                Entry::Card(card) => Some(card),
                _ => None,
            })
            .collect())
    }

    /// List every thread file.
    pub fn list_threads(&self) -> Result<Vec<Thread>> {
        Ok(self
            .list_entries()?
            .into_iter()
            .filter_map(|e| match e {
                Entry::Thread(thread) => Some(thread),
                _ => None,
            })
            .collect())
    }

    pub fn read_card(&self, id: &str) -> Result<Card> {
        let content = std::fs::read_to_string(self.path_for(id))
            .with_context(|| format!("card {id} not found"))?;
        match classify(&content)? {
            Entry::Card(card) => Ok(card),
            _ => Err(anyhow!("{id} is not a card")),
        }
    }

    pub fn read_thread(&self, id: &str) -> Result<Thread> {
        let content = std::fs::read_to_string(self.path_for(id))
            .with_context(|| format!("thread {id} not found"))?;
        match classify(&content)? {
            Entry::Thread(thread) => Ok(thread),
            _ => Err(anyhow!("{id} is not a thread")),
        }
    }

    /// Upsert a card by id.
    pub fn write_card(&self, card: &Card) -> Result<()> {
        let content = serialize_card(card)?;
        std::fs::write(self.path_for(&card.id), content)
            .with_context(|| format!("cannot write card {}", card.id))?;
        Ok(())
    }

    /// Upsert a thread by id.
    pub fn write_thread(&self, thread: &Thread) -> Result<()> {
        let content = serialize_thread(thread)?;
        std::fs::write(self.path_for(&thread.id), content)
            .with_context(|| format!("cannot write thread {}", thread.id))?;
        Ok(())
    }

    /// Create a new card, generating its opaque id and creation timestamp.
    pub fn create_card(&self, body: &str, tags: Vec<String>) -> Result<Card> {
        let card = Card {
            id: uuid::Uuid::new_v4().to_string(),
            title: None,
            created: chrono::Utc::now().to_rfc3339(),
            tags,
            body: body.to_string(),
        };
        self.write_card(&card)?;
        Ok(card)
    }

    /// Create a new thread, generating its opaque id and creation timestamp.
    pub fn create_thread(
        &self,
        title: &str,
        refs: Vec<String>,
        tags: Vec<String>,
        manifest: Vec<ManifestNode>,
    ) -> Result<Thread> {
        let thread = Thread {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            tags,
            refs,
            manifest,
        };
        self.write_thread(&thread)?;
        Ok(thread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        (dir, v)
    }

    #[test]
    fn round_trips_a_note_through_disk() {
        let (_d, v) = vault();
        let mut note = v
            .create_note("Attention Is All You Need", vec!["arXiv:1706.03762".into()], vec!["Transformer".into()], vec!["nlp".into()], "Self-attention.\n")
            .unwrap();
        note.links.push(Link { to: "other-id".into(), why: "introduces the architecture it builds on".into() });
        v.write_note(&note).unwrap();

        let read = v.read_note(&note.id).unwrap();
        assert_eq!(read, note);
    }

    #[test]
    fn rename_preserves_id_and_links() {
        let (_d, v) = vault();
        let mut note = v.create_note("Old Title", vec![], vec![], vec![], "body").unwrap();
        note.links.push(Link { to: "x".into(), why: "because".into() });
        v.write_note(&note).unwrap();

        let renamed = v.rename_title(&note.id, "New Title").unwrap();
        assert_eq!(renamed.id, note.id);
        assert_eq!(renamed.title, "New Title");
        assert_eq!(renamed.links, note.links);
    }

    #[test]
    fn create_yields_a_parseable_file() {
        let (_d, v) = vault();
        let note = v.create_note("Foo", vec![], vec![], vec![], "hello").unwrap();
        let listed = v.list_notes().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, note.id);
        assert!(!note.created.is_empty());
    }

    #[test]
    fn skips_malformed_files_without_panicking() {
        let (dir, v) = vault();
        v.create_note("Good", vec![], vec![], vec![], "ok").unwrap();
        std::fs::write(dir.path().join("junk.md"), "not frontmatter at all").unwrap();
        std::fs::write(dir.path().join("partial.md"), "---\nid: \n---\nbody").unwrap();

        let notes = v.list_notes().unwrap();
        assert_eq!(notes.len(), 1, "only the good note should survive");
    }

    #[test]
    fn parse_rejects_content_without_frontmatter() {
        assert!(parse_note("just text").is_err());
    }

    #[test]
    fn card_round_trips_through_disk() {
        let (_d, v) = vault();
        let card = v
            .create_card("Attention is a soft dictionary lookup.\n\nMore detail here.", vec!["nlp".into()])
            .unwrap();
        assert!(card.title.is_none(), "cards are untitled by default");

        let read = v.read_card(&card.id).unwrap();
        assert_eq!(read.id, card.id);
        assert_eq!(read.tags, card.tags);
        assert_eq!(read.title, card.title);
        assert_eq!(read.body.trim_end(), card.body.trim_end());
        // Reading and re-writing is stable (idempotent).
        v.write_card(&read).unwrap();
        assert_eq!(v.read_card(&card.id).unwrap(), read);
        // A card is not a legacy note, so it is skipped by list_notes.
        assert!(v.list_notes().unwrap().is_empty());
        assert_eq!(v.list_cards().unwrap().len(), 1);
    }

    #[test]
    fn thread_round_trips_its_manifest_tree() {
        let (_d, v) = vault();
        let manifest = vec![
            ManifestNode::leaf("card-a"),
            ManifestNode::new("card-b", vec![ManifestNode::leaf("card-c")]),
        ];
        let thread = v
            .create_thread("On attention", vec!["arXiv:1706.03762".into()], vec![], manifest.clone())
            .unwrap();

        let read = v.read_thread(&thread.id).unwrap();
        assert_eq!(read.title, "On attention");
        assert_eq!(read.refs, vec!["arXiv:1706.03762".to_string()]);
        assert_eq!(read.manifest, manifest, "the nested list round-trips to the same tree");
        assert!(v.list_notes().unwrap().is_empty(), "threads are not legacy notes");
        assert_eq!(v.list_threads().unwrap().len(), 1);
    }

    #[test]
    fn manifest_parsing_tolerates_indent_units_and_stray_lines() {
        // 4-space indentation, a heading line, and a blank line — all tolerated.
        let body = "- [[a]]\n\n## notes\n- [[b]]\n    - [[c]]\n";
        let thread = match classify(&format!(
            "---\nkind: thread\nid: t1\ntitle: T\ncreated: now\n---\n{body}"
        ))
        .unwrap()
        {
            Entry::Thread(t) => t,
            _ => panic!("should classify as a thread"),
        };
        assert_eq!(
            thread.manifest,
            vec![
                ManifestNode::leaf("a"),
                ManifestNode::new("b", vec![ManifestNode::leaf("c")]),
            ]
        );
    }

    #[test]
    fn classify_routes_by_kind_and_defaults_to_legacy() {
        let legacy = "---\nid: n1\ntitle: Old\ncreated: now\n---\nbody";
        assert!(matches!(classify(legacy).unwrap(), Entry::Legacy(_)));
        let card = "---\nkind: card\nid: c1\ncreated: now\n---\nan atom";
        assert!(matches!(classify(card).unwrap(), Entry::Card(_)));
        let thread = "---\nkind: thread\nid: t1\ntitle: T\ncreated: now\n---\n- [[c1]]\n";
        assert!(matches!(classify(thread).unwrap(), Entry::Thread(_)));
    }
}
