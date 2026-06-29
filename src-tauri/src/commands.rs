//! Tauri command layer — thin glue exposing the deep modules to the frontend.
//! Intentionally shallow; not unit-tested (the logic lives in the modules below it).

use crate::index::{DueReview, FailedConnection, NodeMeta, OutLink};
use crate::linker::{self, Resolution};
use crate::recall::{self, RecallResult, ReviewReveal};
use crate::state::{AppState, OpenVault};
use crate::bibtex::{self, Source};
use crate::clip;
use crate::daily;
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

/// Open today's daily/fleeting note, creating it the first time it's asked for. Keyed
/// by the date title so a day always resolves to the same note (no duplicates) — the
/// lookup reuses the linker's exact resolver. Capture only: it never creates an edge.
#[tauri::command]
pub fn open_daily_note(state: State<AppState>) -> Result<Note, String> {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let title = daily::title_for(&date);
    with_vault(&state, |ov| {
        if let Resolution::Exact(node) = linker::resolve(&ov.index, &title)? {
            return ov.vault.read_note(&node.id);
        }
        let note = ov
            .vault
            .create_note(&title, vec![], vec![], daily::tags(), &daily::render_body(&date))?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}

/// Write an imported source (paper or clip) into the vault as a note: its refs, tags,
/// and the shared paper-note body. Never creates an edge — connecting it stays in the
/// justified-link flow.
fn create_source_note(state: &AppState, src: Source) -> Result<Note, String> {
    let title = src.title.trim().to_string();
    if title.is_empty() {
        return Err("the imported source has no title".into());
    }
    let body = bibtex::note_body(&src);
    with_vault(state, |ov| {
        let note = ov
            .vault
            .create_note(&title, src.refs.clone(), vec![], src.tags.clone(), &body)?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}

/// Import a citation into a paper note from either a pasted BibTeX entry or an arXiv
/// id/URL. BibTeX is parsed offline; an arXiv id is resolved against the arXiv API. The
/// note carries the refs and a body skeleton — it never carries edges.
#[tauri::command]
pub fn import_citation(state: State<AppState>, input: String) -> Result<Note, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("paste a BibTeX entry or an arXiv id/URL".into());
    }
    let src = if bibtex::looks_like_bibtex(trimmed) {
        bibtex::parse_bibtex(trimmed).map_err(|e| format!("{e:#}"))?
    } else if let Some(id) = bibtex::arxiv_id(trimmed) {
        bibtex::fetch_arxiv(&id).map_err(|e| format!("{e:#}"))?
    } else {
        return Err("couldn't read that as a BibTeX entry or an arXiv id/URL".into());
    };
    create_source_note(&state, src)
}

/// Clip a URL into a note: fetch the page, extract its title and readable text, and
/// record the URL as the note's ref. Capture only — it never creates an edge.
#[tauri::command]
pub fn clip_url(state: State<AppState>, url: String) -> Result<Note, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("a URL is required".into());
    }
    let html = clip::fetch(&url).map_err(|e| format!("{e:#}"))?;
    let src = clip::to_source(&html, &url);
    create_source_note(&state, src)
}
