//! Tauri command layer — thin glue exposing the deep modules to the frontend.
//! Intentionally shallow; not unit-tested (the logic lives in the modules below it).

use crate::index::{DueReview, FailedConnection, NodeMeta, OutLink};
use crate::linker::{self, Resolution};
use crate::recall::{self, RecallResult, ReviewReveal};
use crate::state::{AppState, OpenVault};
use crate::templates;
use crate::vault::Note;
use tauri::State;

fn with_vault<T>(
    state: &AppState,
    f: impl FnOnce(&OpenVault) -> anyhow::Result<T>,
) -> Result<T, String> {
    let guard = state.open.lock().unwrap();
    let ov = guard.as_ref().ok_or_else(|| "no vault is open".to_string())?;
    f(ov).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_saved_vault(state: State<AppState>) -> Option<String> {
    state.settings().vault_path
}

#[tauri::command]
pub fn open_vault(state: State<AppState>, path: String) -> Result<(), String> {
    state.open_vault(&path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn list_notes(state: State<AppState>) -> Result<Vec<NodeMeta>, String> {
    with_vault(&state, |ov| ov.index.nodes())
}

#[tauri::command]
pub fn get_note(state: State<AppState>, id: String) -> Result<Note, String> {
    with_vault(&state, |ov| ov.vault.read_note(&id))
}

#[tauri::command]
pub fn create_note(
    state: State<AppState>,
    title: String,
    refs: Vec<String>,
    aliases: Vec<String>,
    tags: Vec<String>,
    body: String,
) -> Result<Note, String> {
    with_vault(&state, |ov| {
        let note = ov.vault.create_note(&title, refs, aliases, tags, &body)?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}

#[tauri::command]
pub fn save_note(
    state: State<AppState>,
    id: String,
    title: String,
    body: String,
    refs: Vec<String>,
    aliases: Vec<String>,
    tags: Vec<String>,
) -> Result<Note, String> {
    with_vault(&state, |ov| {
        // Preserve links (managed via commit_link), update only editable fields.
        let mut note = ov.vault.read_note(&id)?;
        note.title = title.trim().to_string();
        note.body = body;
        note.refs = refs;
        note.aliases = aliases;
        note.tags = tags;
        ov.vault.write_note(&note)?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}

#[tauri::command]
pub fn delete_note(state: State<AppState>, id: String) -> Result<(), String> {
    with_vault(&state, |ov| {
        ov.vault.delete_note(&id)?;
        ov.index.delete_node(&id)?;
        Ok(())
    })
}

/// Resolve a typed-from-memory title. Returns the note on exact match, or null.
/// Never returns a candidate list.
#[tauri::command]
pub fn resolve_link(state: State<AppState>, attempt: String) -> Result<Option<NodeMeta>, String> {
    with_vault(&state, |ov| {
        Ok(match linker::resolve(&ov.index, &attempt)? {
            Resolution::Exact(node) => Some(node),
            Resolution::NoMatch => None,
        })
    })
}

#[tauri::command]
pub fn commit_link(
    state: State<AppState>,
    from_id: String,
    to_id: String,
    justification: String,
) -> Result<(), String> {
    with_vault(&state, |ov| {
        linker::commit_edge(&ov.vault, &ov.index, &from_id, &to_id, &justification)
    })
}

#[tauri::command]
pub fn outgoing(state: State<AppState>, id: String) -> Result<Vec<OutLink>, String> {
    with_vault(&state, |ov| ov.index.outgoing(&id))
}

/// Restore a faded (decayed) edge by re-justifying it. The justification is required
/// and re-typed from memory — same friction as creating the link — so a decayed
/// connection can only come back by re-stating *why* it exists. Re-writes the
/// justification through the vault, then resets the edge's decay telemetry.
#[tauri::command]
pub fn restore_link(
    state: State<AppState>,
    from_id: String,
    to_id: String,
    justification: String,
) -> Result<(), String> {
    with_vault(&state, |ov| {
        linker::commit_edge(&ov.vault, &ov.index, &from_id, &to_id, &justification)?;
        ov.index.restore_edge(&from_id, &to_id)
    })
}

#[tauri::command]
pub fn submit_recall(
    state: State<AppState>,
    note_id: String,
    guesses: Vec<String>,
) -> Result<RecallResult, String> {
    with_vault(&state, |ov| {
        recall::submit_guesses(&ov.index, &note_id, &guesses)
    })
}

/// Edges due for a spaced-repetition review, most overdue first. Justifications are
/// withheld — see `grade_review`.
#[tauri::command]
pub fn due_reviews(state: State<AppState>) -> Result<Vec<DueReview>, String> {
    with_vault(&state, |ov| ov.index.due_reviews())
}

/// Grade a review of the A→B connection. `recalled` is the user's self-assessment,
/// committed before the justification is revealed; recording it reschedules the edge.
#[tauri::command]
pub fn grade_review(
    state: State<AppState>,
    from_id: String,
    to_id: String,
    recalled: bool,
) -> Result<ReviewReveal, String> {
    with_vault(&state, |ov| {
        recall::grade_review(&ov.index, &from_id, &to_id, recalled)
    })
}

/// The connections the user fails most often, for the "what to review" surface.
/// Justifications are withheld — it points at weak spots, it is not a cheat sheet.
#[tauri::command]
pub fn what_to_review(
    state: State<AppState>,
    limit: i64,
) -> Result<Vec<FailedConnection>, String> {
    with_vault(&state, |ov| ov.index.most_failed_connections(limit))
}

#[tauri::command]
pub fn search(state: State<AppState>, query: String) -> Result<Vec<NodeMeta>, String> {
    with_vault(&state, |ov| ov.index.search(&query))
}

// --- capture bundle (#18): commands that create notes, never edges ---------------

/// The built-in note-body templates. A template pre-fills structure (prompts/headings)
/// only — it is not a capture shortcut, and it never creates edges.
#[tauri::command]
pub fn list_templates() -> Vec<templates::Template> {
    templates::builtin()
}

/// Create a new note from a template: render its body skeleton with the title and
/// today's date, apply its suggested tags, and write it through the vault. Connecting
/// the note afterwards still happens in the justified-link flow.
#[tauri::command]
pub fn create_from_template(
    state: State<AppState>,
    template_id: String,
    title: String,
) -> Result<Note, String> {
    let tpl = templates::by_id(&template_id)
        .ok_or_else(|| format!("unknown template '{template_id}'"))?;
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("a note needs a title".into());
    }
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let body = templates::render(&tpl.body, &title, &date);
    with_vault(&state, |ov| {
        let note = ov.vault.create_note(&title, vec![], vec![], tpl.tags.clone(), &body)?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}
