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

## Kickoff state protocol

This file on `main` is the single source of truth for what's done — future kickoffs
(`kickoff-text-issues`) read it and skip `[x]` items. So:

1. Each issue carries `touches:` (files it owns) and `blocked-by:` so a kickoff can form
   conflict-free lanes from this text alone, without skimming the repo.
2. **When an agent finishes an issue, its final commit ticks the box here** — change `[ ]` to
   `[x]`, append the commit hash, then `git pull --rebase` and push to `main`. This file is
   shared across lanes, so tick it *last* and rebase to avoid clobbering a sibling's tick.
3. Only `[ ]` (todo) and `[~]` (in progress) issues are eligible for a new kickoff.

## v2 backlog (explicitly out of scope for MVP)

- [ ] **#13 Link decay** — unexercised edges fade visually and must be re-justified to restore.
      touches: `src-tauri/src/index.rs`, `recall.rs` · blocked-by: none
- [ ] **#14 Spaced-repetition quizzing** — surface "how does A relate to B?" on a schedule from
      `last_recalled` / `recall_strength`.
      touches: `src-tauri/src/index.rs`, `recall.rs`, `commands.rs` · blocked-by: none
      (shares files with #13 → same lane, run after #13)
- [ ] **#15 "What to review" surface** — list the connections most often failed, driven by
      `recall_log`.
      touches: `src-tauri/src/index.rs`, `commands.rs`, `src/routes/+page.svelte` · blocked-by: #13
- [ ] **#16 CodeMirror Markdown editor** — replace the body `<textarea>` with CodeMirror +
      `[[wiki-link]]` rendering. Do NOT add link autocomplete (see `CONTEXT.md`).
      touches: `package.json`, `src/lib/Editor.svelte` (new), `src/routes/+page.svelte` (1 line)
      · blocked-by: none
- [ ] **#17 Draw-the-edges graph** — user reconstructs the edges from memory, then it's scored
      against reality. The only graph the thesis allows (no auto-drawn readable map).
      touches: `src/routes/graph/+page.svelte` (new), `src/lib/Graph.svelte` (new),
      `src/routes/+page.svelte` (1 line nav) · blocked-by: none
      (⚠ shares the 1-line `+page.svelte` edit with #16 — sequence those two touches)
- [ ] **#18 Capture bundle** — daily/fleeting notes, templates, tags-as-navigation,
      BibTeX/citation import, web clipping. Own epic; split further before kickoff.
      touches: `src-tauri/src/vault.rs`, `index.rs`, `commands.rs`, `src/**` · blocked-by: deferred
