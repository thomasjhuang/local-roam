//! Sources — the PDF library layer (#19).
//!
//! A "source" is a note whose refs include a local PDF path: the Zettelkasten
//! literature-note tier, with the document itself one click away. Reading and
//! locating a PDF is deliberately frictionless (see CONTEXT.md "The reading
//! layer") — the recall thesis governs *connections*, not document access.
//!
//! The productive friction lives at ingest: a dropped PDF only becomes a source
//! once the user writes their own name and a one-sentence idea for it (the
//! generation effect). Import creates a note, never an edge.

use crate::vault::Note;
use serde::Serialize;

/// A source row for the library view: a note that carries a local PDF.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SourceMeta {
    pub id: String,
    pub title: String,
    pub pdf_path: String,
    /// The user's own one-sentence idea (the first prose line of the body).
    pub idea: String,
    pub tags: Vec<String>,
    pub created: String,
}

/// Whether a ref string points at a PDF on disk (as opposed to an arXiv id,
/// DOI, or URL ref).
pub fn is_pdf_path(r: &str) -> bool {
    let r = r.trim();
    r.to_lowercase().ends_with(".pdf") && !r.contains("://")
}

/// The first PDF path among a note's refs, if any.
pub fn pdf_ref(refs: &[String]) -> Option<&str> {
    refs.iter().map(|r| r.trim()).find(|r| is_pdf_path(r))
}

/// Tags applied to imported PDF sources.
pub fn tags() -> Vec<String> {
    vec!["paper".into()]
}

/// The note body for a freshly imported PDF: the user's idea up top, then room
/// to take literature notes. No links section — edges are never pre-created.
pub fn render_body(idea: &str) -> String {
    format!("{}\n\n## Notes\n\n", idea.trim())
}

/// The idea snippet for the library list: the first non-empty, non-heading line
/// of the body.
pub fn idea_snippet(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("")
        .to_string()
}

/// Project the notes that carry a PDF into library rows, newest first (a
/// Zotero-style "date added" ordering).
pub fn sources(notes: &[Note]) -> Vec<SourceMeta> {
    let mut out: Vec<SourceMeta> = notes
        .iter()
        .filter_map(|n| {
            pdf_ref(&n.refs).map(|p| SourceMeta {
                id: n.id.clone(),
                title: n.title.clone(),
                pdf_path: p.to_string(),
                idea: idea_snippet(&n.body),
                tags: n.tags.clone(),
                created: n.created.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use tempfile::TempDir;

    #[test]
    fn is_pdf_path_matches_files_not_urls_or_ids() {
        assert!(is_pdf_path("/Users/me/papers/attention.pdf"));
        assert!(is_pdf_path("  /tmp/UPPER.PDF "));
        assert!(!is_pdf_path("https://example.com/paper.pdf"), "URL refs are clips, not files");
        assert!(!is_pdf_path("arXiv:1706.03762"));
        assert!(!is_pdf_path("doi:10.1000/x"));
        assert!(!is_pdf_path("/tmp/notes.md"));
    }

    #[test]
    fn pdf_ref_finds_the_pdf_among_other_refs() {
        let refs = vec!["arXiv:1706.03762".to_string(), "/tmp/a.pdf".to_string()];
        assert_eq!(pdf_ref(&refs), Some("/tmp/a.pdf"));
        assert_eq!(pdf_ref(&["arXiv:1706.03762".to_string()]), None);
    }

    #[test]
    fn idea_snippet_skips_headings_and_blanks() {
        assert_eq!(idea_snippet("\n## Notes\n\nthe idea line\nmore"), "the idea line");
        assert_eq!(idea_snippet(&render_body("attention replaces recurrence")),
                   "attention replaces recurrence");
        assert_eq!(idea_snippet("## only headings\n"), "");
    }

    #[test]
    fn sources_projects_only_pdf_notes_newest_first() {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let mut old = v
            .create_note("Old paper", vec!["/tmp/old.pdf".into()], vec![], tags(), &render_body("old idea"))
            .unwrap();
        old.created = "2026-01-01T00:00:00Z".into();
        v.write_note(&old).unwrap();
        v.create_note("Plain zettel", vec![], vec![], vec![], "an idea with no pdf").unwrap();
        let mut new = v
            .create_note("New paper", vec!["/tmp/new.pdf".into()], vec![], tags(), &render_body("new idea"))
            .unwrap();
        new.created = "2026-06-01T00:00:00Z".into();
        v.write_note(&new).unwrap();

        let notes = v.list_notes().unwrap();
        let srcs = sources(&notes);
        assert_eq!(srcs.len(), 2, "the zettel without a pdf is not a source");
        assert_eq!(srcs[0].title, "New paper", "newest first");
        assert_eq!(srcs[0].pdf_path, "/tmp/new.pdf");
        assert_eq!(srcs[0].idea, "new idea");
        assert_eq!(srcs[1].title, "Old paper");
    }
}
