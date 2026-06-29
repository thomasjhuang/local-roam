//! RecallSession — friction mechanic #2: recall-before-reveal backlinks.
//!
//! Opening a note must not show what links to it. Instead the user submits the
//! titles they *believe* link here; we score those guesses against the real
//! backlinks, record a recall rep for every true backlink (hit = success, miss =
//! failure), and only then reveal the full set as feedback. Submitting an empty
//! guess list is a valid "give up" — it reveals everything and logs every backlink
//! as a failed recall.

use crate::index::{self, Backlink, Index};
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

/// The feedback from grading one spaced-repetition review.
#[derive(Debug, Serialize, PartialEq)]
pub struct ReviewReveal {
    /// The justification, revealed only *after* the user committed their self-grade.
    pub why: String,
    /// Recall strength after recording this rep.
    pub recall_strength: f64,
    /// Days until this edge becomes due for review again.
    pub next_interval_days: f64,
}

/// Grade a spaced-repetition review of the A→B edge. `recalled` is the user's
/// self-assessment, committed *before* the justification is revealed — you cannot peek
/// and then claim you knew it. Recording the rep reschedules the edge (via its new
/// strength) and feeds the "what to review" failure stats; the stored justification is
/// then returned as feedback. Errors if the edge no longer exists.
pub fn grade_review(
    index: &Index,
    from_id: &str,
    to_id: &str,
    recalled: bool,
) -> Result<ReviewReveal> {
    let (why, _) = index
        .edge(from_id, to_id)?
        .ok_or_else(|| anyhow::anyhow!("no such edge to review"))?;
    index.record_recall(from_id, to_id, recalled)?;
    let strength = index.edge(from_id, to_id)?.map(|(_, s)| s).unwrap_or(0.0);
    Ok(ReviewReveal {
        why,
        recall_strength: strength,
        next_interval_days: index::review_interval_days(strength),
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
    fn grade_review_reveals_justification_and_reschedules() {
        let (_d, idx, a, c, b) = setup();

        // A correct review: reveals the reason, strengthens, lengthens the interval.
        let rev = grade_review(&idx, &a, &b, true).unwrap();
        assert_eq!(rev.why, "reason a");
        assert_eq!(rev.recall_strength, 1.0);
        assert_eq!(rev.next_interval_days, 2.0);

        // A miss reveals the reason too, but weakens it (clamped at 0) — logged as a
        // failure so it resurfaces sooner and feeds "what to review".
        let rev2 = grade_review(&idx, &c, &b, false).unwrap();
        assert_eq!(rev2.why, "reason c");
        assert_eq!(rev2.recall_strength, 0.0);
        assert_eq!(rev2.next_interval_days, 1.0);

        // Reviewing a non-existent edge errors rather than revealing nothing.
        assert!(grade_review(&idx, &a, "no-such-id", true).is_err());
    }

    #[test]
    fn hits_strengthen_recall() {
        let (_d, idx, a, _c, b) = setup();
        submit_guesses(&idx, &b, &["Alpha".into(), "Charlie".into()]).unwrap();
        let strength = idx.backlinks(&b).unwrap().iter().find(|x| x.from_id == a).unwrap().recall_strength;
        assert_eq!(strength, 1.0);
    }
}
