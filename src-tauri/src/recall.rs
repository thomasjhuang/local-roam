//! RecallSession — friction mechanic #2: recall-before-reveal backlinks.
//!
//! Opening a note must not show what links to it. Instead the user submits the
//! titles they *believe* link here; we score those guesses against the real
//! backlinks, record a recall rep for every true backlink (hit = success, miss =
//! failure), and only then reveal the full set as feedback. Submitting an empty
//! guess list is a valid "give up" — it reveals everything and logs every backlink
//! as a failed recall.

use crate::index::{Backlink, Index};
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub struct RecallResult {
    /// True backlinks the user correctly recalled.
    pub hits: Vec<Backlink>,
    /// True backlinks the user failed to recall.
    pub missed: Vec<Backlink>,
    /// Guesses that did not correspond to a real backlink (wrong or unresolved).
    pub spurious: Vec<String>,
    /// The full set of true backlinks, revealed as feedback.
    pub reveal: Vec<Backlink>,
}

/// Score the user's recalled backlinks for `note_id`, record reps, and reveal.
pub fn submit_guesses(index: &Index, note_id: &str, guesses: &[String]) -> Result<RecallResult> {
    let actual = index.backlinks(note_id)?;

    // Resolve each guess to a note id (exact recall only). Track which guesses were
    // "good" (matched a real backlink) so the rest can be reported as spurious.
    let mut guessed_ids = Vec::new();
    let mut spurious = Vec::new();
    for guess in guesses {
        if guess.trim().is_empty() {
            continue;
        }
        match index.find_by_title_or_alias(guess)? {
            Some(node) if actual.iter().any(|b| b.from_id == node.id) => {
                if !guessed_ids.contains(&node.id) {
                    guessed_ids.push(node.id);
                }
            }
            _ => spurious.push(guess.clone()),
        }
    }

    let mut hits = Vec::new();
    let mut missed = Vec::new();
    for b in &actual {
        let success = guessed_ids.contains(&b.from_id);
        index.record_recall(&b.from_id, note_id, success)?;
        if success {
            hits.push(b.clone());
        } else {
            missed.push(b.clone());
        }
    }

    Ok(RecallResult {
        hits,
        missed,
        spurious,
        reveal: actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linker;
    use crate::vault::Vault;
    use tempfile::TempDir;

    /// Two notes (A, C) both link to target B.
    fn setup() -> (TempDir, Index, String, String, String) {
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let b = v.create_note("Target", vec![], vec![], vec![], "").unwrap();
        let a = v.create_note("Alpha", vec![], vec![], vec![], "").unwrap();
        let c = v.create_note("Charlie", vec![], vec![], vec![], "").unwrap();
        let idx = Index::open_in_memory().unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        linker::commit_edge(&v, &idx, &a.id, &b.id, "reason a").unwrap();
        linker::commit_edge(&v, &idx, &c.id, &b.id, "reason c").unwrap();
        (dir, idx, a.id, c.id, b.id)
    }

    #[test]
    fn all_correct_guesses_are_all_hits() {
        let (_d, idx, _a, _c, b) = setup();
        let r = submit_guesses(&idx, &b, &["Alpha".into(), "Charlie".into()]).unwrap();
        assert_eq!(r.hits.len(), 2);
        assert!(r.missed.is_empty());
        assert!(r.spurious.is_empty());
        assert_eq!(r.reveal.len(), 2);
    }

    #[test]
    fn partial_recall_splits_hits_and_misses() {
        let (_d, idx, _a, _c, b) = setup();
        let r = submit_guesses(&idx, &b, &["Alpha".into()]).unwrap();
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].from_title, "Alpha");
        assert_eq!(r.missed.len(), 1);
        assert_eq!(r.missed[0].from_title, "Charlie");
    }

    #[test]
    fn wrong_or_unknown_guesses_are_spurious() {
        let (_d, idx, _a, _c, b) = setup();
        let r = submit_guesses(&idx, &b, &["Target".into(), "Nonexistent".into()]).unwrap();
        // "Target" resolves but isn't a backlink; "Nonexistent" doesn't resolve.
        assert_eq!(r.spurious.len(), 2);
        assert!(r.hits.is_empty());
        assert_eq!(r.missed.len(), 2);
    }

    #[test]
    fn giving_up_reveals_all_and_records_failures() {
        let (_d, idx, a, _c, b) = setup();
        let r = submit_guesses(&idx, &b, &[]).unwrap();
        assert_eq!(r.missed.len(), 2);
        assert_eq!(r.reveal.len(), 2);
        // failure weakened (clamped at 0, started at 0).
        assert_eq!(idx.backlinks(&b).unwrap().iter().find(|x| x.from_id == a).unwrap().recall_strength, 0.0);
    }

    #[test]
    fn hits_strengthen_recall() {
        let (_d, idx, a, _c, b) = setup();
        submit_guesses(&idx, &b, &["Alpha".into(), "Charlie".into()]).unwrap();
        let strength = idx.backlinks(&b).unwrap().iter().find(|x| x.from_id == a).unwrap().recall_strength;
        assert_eq!(strength, 1.0);
    }
}
