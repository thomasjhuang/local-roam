//! Tauri command layer — thin glue exposing the deep modules to the frontend.
//! Intentionally shallow; not unit-tested (the logic lives in the modules below it).

use crate::index::{Backlink, CardMembership, CardMeta, NodeMeta, OutLink, TagCount, ThreadCard, ThreadMeta};
use crate::linker::{self, Resolution};
use crate::state::{AppState, OpenVault};
use crate::bibtex::{self, Source};
use crate::clip;
use crate::daily;
use crate::sources::{self, SourceMeta};
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

/// The notes that link to this one, shown directly (the recall gate is retired).
#[tauri::command]
pub fn backlinks(state: State<AppState>, id: String) -> Result<Vec<Backlink>, String> {
    with_vault(&state, |ov| ov.index.backlinks(&id))
}

#[tauri::command]
pub fn search(state: State<AppState>, query: String) -> Result<Vec<NodeMeta>, String> {
    with_vault(&state, |ov| ov.index.search(&query))
}

/// Every tag with its note count, for the tag-browsing escape hatch (#18c). Like
/// `search`, this is navigation only — present but not the default path; it never
/// creates a note or an edge.
#[tauri::command]
pub fn list_tags(state: State<AppState>) -> Result<Vec<TagCount>, String> {
    with_vault(&state, |ov| ov.index.tags())
}

/// The notes carrying a tag (exact, case-insensitive). Browsing, not capture: it
/// surfaces existing notes and creates nothing.
#[tauri::command]
pub fn notes_by_tag(state: State<AppState>, tag: String) -> Result<Vec<NodeMeta>, String> {
    with_vault(&state, |ov| ov.index.notes_with_tag(&tag))
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

// --- sources library (#19): the reading layer -------------------------------------
// Reading and locating a PDF is frictionless by design (CONTEXT.md "The reading
// layer"); the friction lives at ingest (a self-written name + idea) and, as always,
// in connecting: import creates a note, never an edge.

/// The library rows: every note that carries a local PDF ref, newest first.
#[tauri::command]
pub fn list_sources(state: State<AppState>) -> Result<Vec<SourceMeta>, String> {
    with_vault(&state, |ov| Ok(sources::sources(&ov.vault.list_notes()?)))
}

/// Turn a dropped PDF into a source note. The name and idea are the user's own —
/// the generation-effect friction that replaces pasting a citation. Idempotent per
/// path: re-dropping an already-imported PDF returns its existing note.
#[tauri::command]
pub fn import_pdf_source(
    state: State<AppState>,
    path: String,
    name: String,
    idea: String,
) -> Result<Note, String> {
    let path = path.trim().to_string();
    let name = name.trim().to_string();
    let idea = idea.trim().to_string();
    if !sources::is_pdf_path(&path) {
        return Err("that file is not a PDF".into());
    }
    if !std::path::Path::new(&path).is_file() {
        return Err(format!("no PDF found at {path}"));
    }
    if name.is_empty() {
        return Err("name the paper in your own words".into());
    }
    if idea.is_empty() {
        return Err("write one sentence: what is this paper to you?".into());
    }
    with_vault(&state, |ov| {
        if let Some(existing) = ov
            .vault
            .list_notes()?
            .into_iter()
            .find(|n| sources::pdf_ref(&n.refs) == Some(path.as_str()))
        {
            return Ok(existing);
        }
        let note = ov
            .vault
            .create_note(&name, vec![path.clone()], vec![], sources::tags(), &sources::render_body(&idea))?;
        ov.index.reindex_note(&note)?;
        Ok(note)
    })
}

/// Open a source's PDF in the system viewer. Pure reading — no recall gate.
#[tauri::command]
pub fn open_source(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let note = with_vault(&state, |ov| ov.vault.read_note(&id))?;
    let path = sources::pdf_ref(&note.refs)
        .ok_or_else(|| format!("\"{}\" has no PDF attached", note.title))?;
    if !std::path::Path::new(path).is_file() {
        return Err(format!("the PDF has moved or was deleted: {path}"));
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("{e:#}"))
}

// --- v3 card/thread model (#22): read surfaces for the new UI (#23+) ----------------
// The vault is the source of truth; these expose the derived card/thread/membership
// cache. Folgezettel addresses come back derived, never stored.

/// Every thread (papers + idea threads), title-sorted, with its card count.
#[tauri::command]
pub fn list_threads(state: State<AppState>) -> Result<Vec<ThreadMeta>, String> {
    with_vault(&state, |ov| ov.index.threads())
}

/// The cards of a thread in manifest order, each with its derived Folgezettel address.
#[tauri::command]
pub fn thread_cards(state: State<AppState>, thread_id: String) -> Result<Vec<ThreadCard>, String> {
    with_vault(&state, |ov| ov.index.thread_cards(&thread_id))
}

/// Every card, with its first-line label.
#[tauri::command]
pub fn list_cards(state: State<AppState>) -> Result<Vec<CardMeta>, String> {
    with_vault(&state, |ov| ov.index.cards())
}

/// Every thread a card belongs to, with its derived address there (a card in two
/// threads returns two — two addresses for one card).
#[tauri::command]
pub fn card_memberships(state: State<AppState>, card_id: String) -> Result<Vec<CardMembership>, String> {
    with_vault(&state, |ov| ov.index.card_memberships(&card_id))
}

/// The ids a card links to (parsed from its body wiki-links); a target may be a card
/// or a thread.
#[tauri::command]
pub fn card_targets(state: State<AppState>, card_id: String) -> Result<Vec<String>, String> {
    with_vault(&state, |ov| ov.index.card_targets(&card_id))
}
