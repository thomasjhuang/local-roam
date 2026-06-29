//! Linker — friction mechanic #1: link-from-memory with a mandatory justification.
//!
//! `resolve` takes a title the user typed *from memory* and returns either an exact
//! match or nothing. It deliberately never returns a candidate list: this is recall,
//! not recognition. `commit_edge` refuses to create a link without a justification —
//! no connection may exist in the vault without an understood reason.

use crate::index::{Index, NodeMeta};
use crate::vault::{Link, Vault};
use anyhow::{anyhow, Result};

/// The result of resolving a typed-from-memory title.
#[derive(Debug, PartialEq)]
pub enum Resolution {
    Exact(NodeMeta),
    NoMatch,
}

/// Resolve a recalled title to a single note, or nothing. No fuzzy matching, no
/// candidate list — the user must reproduce the title (or an alias) correctly.
pub fn resolve(index: &Index, attempt: &str) -> Result<Resolution> {
    match index.find_by_title_or_alias(attempt)? {
        Some(node) => Ok(Resolution::Exact(node)),
        None => Ok(Resolution::NoMatch),
    }
}

/// Commit a justified, directional edge from `from_id` to `to_id`. The justification
/// must be non-empty. Writes the link into the source note's frontmatter (the source
/// of truth) and reindexes it.
pub fn commit_edge(
    vault: &Vault,
    index: &Index,
    from_id: &str,
    to_id: &str,
    justification: &str,
) -> Result<()> {
    let why = justification.trim();
    if why.is_empty() {
        return Err(anyhow!("a link requires a one-sentence justification"));
    }
    if from_id == to_id {
        return Err(anyhow!("a note cannot link to itself"));
    }

    let mut note = vault.read_note(from_id)?;
    match note.links.iter_mut().find(|l| l.to == to_id) {
        Some(existing) => existing.why = why.to_string(),
        None => note.links.push(Link {
            to: to_id.to_string(),
            why: why.to_string(),
        }),
    }
    vault.write_note(&note)?;
    index.reindex_note(&note)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Vault, Index, String, String) {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let a = v.create_note("Source", vec![], vec![], vec![], "").unwrap();
        let b = v.create_note("Target", vec![], vec!["Goal".into()], vec![], "").unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        (dir, v, idx, a.id, b.id)
    }

    #[test]
    fn resolve_returns_exact_on_correct_recall() {
        let (_d, _v, idx, _a, b_id) = setup();
        assert_eq!(resolve(&idx, "Target").unwrap(), Resolution::Exact(idx.find_by_title_or_alias("Target").unwrap().unwrap()));
        // alias also works
        match resolve(&idx, "goal").unwrap() {
            Resolution::Exact(n) => assert_eq!(n.id, b_id),
            _ => panic!("alias should resolve"),
        }
    }

    #[test]
    fn resolve_returns_nomatch_on_near_miss() {
        let (_d, _v, idx, _a, _b) = setup();
        assert_eq!(resolve(&idx, "Targ").unwrap(), Resolution::NoMatch);
        assert_eq!(resolve(&idx, "").unwrap(), Resolution::NoMatch);
    }

    #[test]
    fn commit_edge_rejects_empty_justification() {
        let (_d, v, idx, a_id, b_id) = setup();
        assert!(commit_edge(&v, &idx, &a_id, &b_id, "   ").is_err());
        assert!(commit_edge(&v, &idx, &a_id, &b_id, "").is_err());
        // nothing was written
        assert!(v.read_note(&a_id).unwrap().links.is_empty());
        assert!(idx.backlinks(&b_id).unwrap().is_empty());
    }

    #[test]
    fn commit_edge_writes_justified_link_to_vault_and_index() {
        let (_d, v, idx, a_id, b_id) = setup();
        commit_edge(&v, &idx, &a_id, &b_id, "builds directly on it").unwrap();

        let note = v.read_note(&a_id).unwrap();
        assert_eq!(note.links.len(), 1);
        assert_eq!(note.links[0].why, "builds directly on it");

        let back = idx.backlinks(&b_id).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].from_id, a_id);
    }

    #[test]
    fn commit_edge_rejects_self_link() {
        let (_d, v, idx, a_id, _b) = setup();
        assert!(commit_edge(&v, &idx, &a_id, &a_id, "because").is_err());
    }

    #[test]
    fn commit_edge_updates_existing_justification_without_duplicating() {
        let (_d, v, idx, a_id, b_id) = setup();
        commit_edge(&v, &idx, &a_id, &b_id, "first reason").unwrap();
        commit_edge(&v, &idx, &a_id, &b_id, "better reason").unwrap();
        let note = v.read_note(&a_id).unwrap();
        assert_eq!(note.links.len(), 1);
        assert_eq!(note.links[0].why, "better reason");
    }
}
