//! Tauri command layer — thin glue exposing the deep modules to the frontend.
//! Intentionally shallow; not unit-tested (the logic lives in the modules below it).

use crate::index::{NodeMeta, OutLink};
use crate::linker::{self, Resolution};
use crate::recall::{self, RecallResult};
use crate::state::{AppState, OpenVault};
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

#[tauri::command]
pub fn search(state: State<AppState>, query: String) -> Result<Vec<NodeMeta>, String> {
    with_vault(&state, |ov| ov.index.search(&query))
}
