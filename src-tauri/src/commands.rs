//! Tauri command layer — thin glue exposing the deep modules to the frontend.
//! Intentionally shallow; not unit-tested (the logic lives in the modules below it).

use crate::index::{Backlink, CardMembership, CardMeta, CardRef, NodeMeta, OutLink, TagCount, ThreadCard, ThreadMeta};
use crate::linker::{self, Resolution};
use crate::state::{AppState, OpenVault};
use crate::bibtex::{self, Source};
use crate::clip;
use crate::daily;
use crate::folgezettel::ManifestNode;
use crate::sources::{self, SourceMeta};
use crate::templates;
use crate::vault::{self, Note};
use anyhow::anyhow;
use serde::Serialize;
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

// --- v3 thread view + card editor (#23): write surfaces --------------------------------
// The vault is the source of truth: every mutation writes card/thread files, then rebuilds
// the derived index from the vault (the index is a cache — see CONTEXT.md "Architecture").
// The placement gesture (continue / branch / new-thread) maintains the manifest silently.

/// One card as it reads inside a thread: its derived address + position + full body.
#[derive(Serialize)]
pub struct ThreadCardFull {
    pub card_id: String,
    pub address: String,
    pub position: i64,
    pub label: String,
    pub body: String,
}

/// A thread ready to read/write: its title, refs, and its cards in manifest order with
/// their bodies concatenated by the UI into flowing prose.
#[derive(Serialize)]
pub struct ThreadFull {
    pub id: String,
    pub title: String,
    pub refs: Vec<String>,
    pub cards: Vec<ThreadCardFull>,
}

/// The result of splitting/adding a card: the new card's id and the thread it landed in
/// (its own thread for the new-thread placement).
#[derive(Serialize)]
pub struct PlacementResult {
    pub card_id: String,
    pub thread_id: String,
}

/// A thread ready to read and write: title/refs from the vault (source of truth), cards
/// in manifest (address) order with their full bodies. A dangling manifest link (a card
/// whose file is missing) survives as an empty body rather than failing the whole read.
#[tauri::command]
pub fn get_thread(state: State<AppState>, thread_id: String) -> Result<ThreadFull, String> {
    with_vault(&state, |ov| {
        let thread = ov.vault.read_thread(&thread_id)?;
        let cards = ov
            .index
            .thread_cards(&thread_id)?
            .into_iter()
            .map(|c| ThreadCardFull {
                body: ov.vault.read_card(&c.card_id).map(|k| k.body).unwrap_or_default(),
                card_id: c.card_id,
                address: c.address,
                position: c.position,
                label: c.label,
            })
            .collect();
        Ok(ThreadFull {
            id: thread.id,
            title: thread.title,
            refs: thread.refs,
            cards,
        })
    })
}

/// Write a card's body through the vault (writing *through* the card, per #23), then
/// re-sync the index. Only the body changes; the card's id and creation time are stable.
#[tauri::command]
pub fn save_card(state: State<AppState>, card_id: String, body: String) -> Result<(), String> {
    with_vault(&state, |ov| {
        let mut card = ov.vault.read_card(&card_id)?;
        card.body = body;
        ov.vault.write_card(&card)?;
        ov.index.rebuild_from_vault(&ov.vault)?;
        Ok(())
    })
}

/// Rename a thread (threads are titled; cards are not). Preserves its id and manifest.
#[tauri::command]
pub fn rename_thread(state: State<AppState>, thread_id: String, title: String) -> Result<(), String> {
    with_vault(&state, |ov| {
        let mut thread = ov.vault.read_thread(&thread_id)?;
        thread.title = title.trim().to_string();
        ov.vault.write_thread(&thread)?;
        ov.index.rebuild_from_vault(&ov.vault)?;
        Ok(())
    })
}

/// Create a new, empty thread (no cards yet). Returns its id.
#[tauri::command]
pub fn new_thread(state: State<AppState>, title: String) -> Result<String, String> {
    with_vault(&state, |ov| {
        if title.trim().is_empty() {
            return Err(anyhow!("a thread needs a title"));
        }
        let thread = ov.vault.create_thread(title.trim(), vec![], vec![], vec![])?;
        ov.index.rebuild_from_vault(&ov.vault)?;
        Ok(thread.id)
    })
}

/// Place `card` in `thread`'s manifest per `placement`, appending to the trunk end when
/// the anchor is absent (an empty thread, or the bottom "+ card" affordance).
fn place_in_manifest(
    thread: &mut crate::vault::Thread,
    anchor_card_id: Option<&str>,
    placement: &str,
    new_card_id: &str,
) {
    let placed = match anchor_card_id {
        Some(anchor) if placement == "branch" => {
            vault::insert_child(&mut thread.manifest, anchor, new_card_id)
        }
        Some(anchor) => vault::insert_sibling_after(&mut thread.manifest, anchor, new_card_id),
        None => false,
    };
    if !placed {
        thread.manifest.push(ManifestNode::leaf(new_card_id));
    }
}

/// Add a fresh card to a thread via the placement gesture: `continue` (a sibling after the
/// anchor at its own depth) or `branch` (a child of the anchor). A `None` anchor appends to
/// the trunk end. The manifest is rewritten silently. Returns the new card's id.
#[tauri::command]
pub fn add_card(
    state: State<AppState>,
    thread_id: String,
    anchor_card_id: Option<String>,
    placement: String,
    body: String,
) -> Result<String, String> {
    with_vault(&state, |ov| {
        let card = ov.vault.create_card(&body, vec![])?;
        let mut thread = ov.vault.read_thread(&thread_id)?;
        place_in_manifest(&mut thread, anchor_card_id.as_deref(), &placement, &card.id);
        ov.vault.write_thread(&thread)?;
        ov.index.rebuild_from_vault(&ov.vault)?;
        Ok(card.id)
    })
}

/// Split a card at the cursor: the `head` stays in the source card, the `tail` becomes a
/// new card placed by the gesture — `continue`/`branch` in the same thread, or `new_thread`
/// (the tail starts a fresh thread titled `new_thread_title`). Returns the new card and the
/// thread it landed in.
#[tauri::command]
pub fn split_card(
    state: State<AppState>,
    thread_id: String,
    source_card_id: String,
    head: String,
    tail: String,
    placement: String,
    new_thread_title: Option<String>,
) -> Result<PlacementResult, String> {
    with_vault(&state, |ov| {
        // The head stays with the source card; the tail is lifted into a new card.
        let mut source = ov.vault.read_card(&source_card_id)?;
        source.body = head;
        ov.vault.write_card(&source)?;
        let card = ov.vault.create_card(&tail, vec![])?;

        let dest_thread = if placement == "new_thread" {
            let title = new_thread_title.unwrap_or_default();
            if title.trim().is_empty() {
                return Err(anyhow!("a new thread needs a title"));
            }
            let thread = ov.vault.create_thread(
                title.trim(),
                vec![],
                vec![],
                vec![ManifestNode::leaf(card.id.as_str())],
            )?;
            thread.id
        } else {
            let mut thread = ov.vault.read_thread(&thread_id)?;
            place_in_manifest(&mut thread, Some(&source_card_id), &placement, &card.id);
            ov.vault.write_thread(&thread)?;
            thread.id
        };
        ov.index.rebuild_from_vault(&ov.vault)?;
        Ok(PlacementResult { card_id: card.id, thread_id: dest_thread })
    })
}

/// Merge a card up into the card before it in reading order (the inverse of split, for a
/// fluid boundary): appends this card's body to the previous card, drops it from *this*
/// thread's manifest (its branches splice up, nothing lost), and deletes the card file only
/// if it now belongs to no thread. Returns the id of the card the text merged into, or
/// `None` when the card is first in the thread (nothing above to merge into).
#[tauri::command]
pub fn merge_card_up(
    state: State<AppState>,
    thread_id: String,
    card_id: String,
) -> Result<Option<String>, String> {
    with_vault(&state, |ov| {
        let ordered = ov.index.thread_cards(&thread_id)?;
        let Some(pos) = ordered.iter().position(|c| c.card_id == card_id) else {
            return Ok(None);
        };
        if pos == 0 {
            return Ok(None);
        }
        let prev_id = ordered[pos - 1].card_id.clone();

        let cur_body = ov.vault.read_card(&card_id)?.body.trim().to_string();
        if !cur_body.is_empty() {
            let mut prev = ov.vault.read_card(&prev_id)?;
            prev.body = if prev.body.trim_end().is_empty() {
                cur_body
            } else {
                format!("{}\n\n{}", prev.body.trim_end(), cur_body)
            };
            ov.vault.write_card(&prev)?;
        }

        let mut thread = ov.vault.read_thread(&thread_id)?;
        vault::remove_node(&mut thread.manifest, &card_id);
        ov.vault.write_thread(&thread)?;
        ov.index.rebuild_from_vault(&ov.vault)?;

        // Delete the card file only if no thread still references it.
        if ov.index.card_memberships(&card_id)?.is_empty() {
            ov.vault.delete_note(&card_id)?;
            ov.index.rebuild_from_vault(&ov.vault)?;
        }
        Ok(Some(prev_id))
    })
}

/// The cards that link to a target (a card or a thread), each with its first-line label
/// and one thread it lives in — the hand-made backlink shown with context (#23).
#[tauri::command]
pub fn card_backlinks(state: State<AppState>, target_id: String) -> Result<Vec<CardRef>, String> {
    with_vault(&state, |ov| ov.index.backlinks_to(&target_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;
    use crate::vault::Vault;
    use tempfile::TempDir;

    fn addr(v: &Vault, idx: &Index, tid: &str, card: &str) -> String {
        idx.rebuild_from_vault(v).unwrap();
        idx.thread_cards(tid)
            .unwrap()
            .into_iter()
            .find(|c| c.card_id == card)
            .unwrap()
            .address
    }

    #[test]
    fn placement_gesture_routes_continue_branch_and_trunk_append() {
        // A thread [a, b]. Continue at "a" → sibling between a and b (address 2). Branch at
        // "b" → child of b (3a). No anchor → trunk end.
        let mut thread = crate::vault::Thread {
            id: "t".into(),
            title: "T".into(),
            created: "now".into(),
            tags: vec![],
            refs: vec![],
            manifest: vec![ManifestNode::leaf("a"), ManifestNode::leaf("b")],
        };
        place_in_manifest(&mut thread, Some("a"), "continue", "cont");
        place_in_manifest(&mut thread, Some("b"), "branch", "br");
        place_in_manifest(&mut thread, None, "continue", "end");

        let pairs: Vec<(String, String)> = crate::folgezettel::addresses(&thread.manifest)
            .into_iter()
            .map(|x| (x.card_id, x.address))
            .collect();
        assert!(pairs.contains(&("cont".into(), "2".into())), "continue → 2");
        assert!(pairs.contains(&("br".into(), "3a".into())), "branch → 3a");
        assert!(pairs.contains(&("end".into(), "4".into())), "no anchor → trunk end");
    }

    #[test]
    fn split_then_merge_round_trips_the_body_and_addresses() {
        // Drive the exact vault+index sequence the split/merge commands run, proving the
        // make-or-break write path end-to-end (no Tauri State needed).
        let dir = TempDir::new().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let idx = Index::open_in_memory().unwrap();

        let src = v.create_card("Head half.\n\nTail half.", vec![]).unwrap();
        let thread = v
            .create_thread("On splitting", vec![], vec![], vec![ManifestNode::leaf(&src.id)])
            .unwrap();

        // --- split at the blank line: head stays, tail becomes a "continue" sibling ---
        let mut source = v.read_card(&src.id).unwrap();
        source.body = "Head half.".into();
        v.write_card(&source).unwrap();
        let tail = v.create_card("Tail half.", vec![]).unwrap();
        let mut t = v.read_thread(&thread.id).unwrap();
        vault::insert_sibling_after(&mut t.manifest, &src.id, &tail.id);
        v.write_thread(&t).unwrap();

        assert_eq!(addr(&v, &idx, &thread.id, &src.id), "1");
        assert_eq!(addr(&v, &idx, &thread.id, &tail.id), "2", "tail continues the thread");
        assert_eq!(v.read_card(&src.id).unwrap().body.trim(), "Head half.");

        // --- merge the tail back up: its body appends to the head, the tail card is gone ---
        let mut prev = v.read_card(&src.id).unwrap();
        prev.body = format!("{}\n\n{}", prev.body.trim_end(), v.read_card(&tail.id).unwrap().body.trim());
        v.write_card(&prev).unwrap();
        let mut t = v.read_thread(&thread.id).unwrap();
        vault::remove_node(&mut t.manifest, &tail.id);
        v.write_thread(&t).unwrap();
        idx.rebuild_from_vault(&v).unwrap();
        assert!(idx.card_memberships(&tail.id).unwrap().is_empty(), "tail orphaned by the merge");
        v.delete_note(&tail.id).unwrap();
        idx.rebuild_from_vault(&v).unwrap();

        let cards = idx.thread_cards(&thread.id).unwrap();
        assert_eq!(cards.len(), 1, "back to a single card after merge");
        assert_eq!(v.read_card(&src.id).unwrap().body.trim_end(), "Head half.\n\nTail half.");
    }
}
