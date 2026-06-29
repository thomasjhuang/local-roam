//! Vault — owns the on-disk Markdown files and frontmatter (de)serialization.
//!
//! This is the ONLY module that knows the file format. Every note is one Markdown
//! file named `<id>.md` with a YAML frontmatter block. The `id` is the source of
//! truth and never changes, so titles can be renamed without breaking links.
//!
//! Edges are authored from the *source* note and stored in its frontmatter `links`
//! list (each with a mandatory `why` justification). This keeps the vault the single
//! source of truth — the SQLite Index is always rebuildable from these files.

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

    /// List every parseable note. Malformed files are skipped, never fatal.
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut notes = Vec::new();
        for entry in WalkDir::new(&self.root)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            match parse_note(&content) {
                Ok(note) => notes.push(note),
                Err(err) => eprintln!("skipping malformed note {}: {err:#}", path.display()),
            }
        }
        Ok(notes)
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
    pub fn rename_title(&self, id: &str, new_title: &str) -> Result<Note> {
        let mut note = self.read_note(id)?;
        note.title = new_title.trim().to_string();
        self.write_note(&note)?;
        Ok(note)
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
}
