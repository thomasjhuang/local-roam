# local-roam — Issue Tracker

Ordered task list for the MVP described in `docs/PRD.md`. Work top-to-bottom.
Mark `[x]` when done and reference the commit. v2 backlog at the bottom.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done

## MVP (v1) — COMPLETE (21/21 Rust tests pass, frontend type-checks clean)

- [x] **#1 Scaffold** — Tauri 2 + SvelteKit-TS project; Tauri CLI as dev-dep; app boots.
- [x] **#2 Vault module (Rust)** — frontmatter model (`id`, `title`, `created`, `aliases`,
      `tags`, `refs`, `links`) + Markdown body; `list_notes`, `read_note`, `write_note`,
      `create_note`, `delete_note`, `rename_title`. Tolerates malformed frontmatter (skips,
      never panics). *Tested: round-trip, rename preserves id, create yields parseable file,
      malformed skipped.* — `src-tauri/src/vault.rs`
- [x] **#3 Index module (Rust)** — SQLite mirror; `rebuild_from_vault` (syncs, preserving recall
      telemetry), `nodes`, `backlinks`, `outgoing`, `reindex_note`, `delete_node`,
      `record_recall`, `search`, `find_by_title_or_alias`. Edge row carries `justification`,
      `last_recalled`, `recall_strength`; `recall_log` table records every rep. *Tested: query
      correctness vs fixture, idempotent rebuild, telemetry survives rebuild, exact (non-fuzzy)
      lookup, search.* — `src-tauri/src/index.rs`
- [x] **#4 Linker module (Rust)** — `resolve(attempt) -> Exact(NodeMeta) | NoMatch` (NO candidate
      leak); `commit_edge` rejects empty justification + self-links, updates-not-duplicates.
      *Tested: exact + alias resolve, near-miss NoMatch, empty justification rejected, self-link
      rejected.* — `src-tauri/src/linker.rs`
- [x] **#5 RecallSession module (Rust)** — `submit_guesses` -> scored hits/missed/spurious + full
      reveal; records a recall rep (success AND failure) per true backlink. Empty guesses = give
      up. *Tested: all-correct, partial, spurious, give-up, hits strengthen.* — `src-tauri/src/recall.rs`
- [x] **#6 Tauri command layer** — thin commands; app-state holds the open vault. — `commands.rs`, `state.rs`
- [x] **#7 Vault picker + persistence** — folder picker (plugin-dialog); path persisted to
      settings, reopens on launch; empty-vault welcome screen. — `settings.rs`, `+page.svelte`
- [x] **#8 Note list + editor** — sidebar list; editor for body + title/refs/aliases/tags; save
      writes through Vault and reindexes. NOTE: uses a styled textarea, not CodeMirror — see
      v2 backlog for the CodeMirror upgrade.
- [x] **#9 Friction link flow (mechanic #1)** — "link from memory" input, NO autocomplete;
      mandatory justification before commit; NoMatch offers retry or deliberate new-note.
- [x] **#10 Recall-before-reveal backlinks (mechanic #2)** — guess prompt first, score, then
      reveal with justifications; missed backlinks highlighted.
- [x] **#11 Search escape hatch** — full-text search over title/alias/body, collapsed under an
      "escape hatch" disclosure.
- [x] **#12 CONTEXT.md + README** — `CONTEXT.md` (design thesis), `README.md` (run/test).

## v2 backlog (explicitly out of scope for MVP)

- [ ] Draw-the-edges-from-memory graph reconstruction (scored against reality).
- [ ] Spaced-repetition quizzing of relationships (uses `last_recalled` / `recall_strength`).
- [ ] Link decay — unexercised edges fade and must be re-justified.
- [ ] "What to review" surface driven by `recall_log` (failed recall attempts are the signal).
- [ ] CodeMirror Markdown editor (replace the textarea) with `[[wiki-link]]` rendering.
- [ ] Daily/fleeting notes; templates; tags-as-navigation; BibTeX/citation import; web clipping.
