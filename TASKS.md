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

- [x] **#13 Link decay** — unexercised edges fade visually and must be re-justified to restore.
      touches: `src-tauri/src/index.rs`, `recall.rs` · blocked-by: none
      Done: `index.rs` exposes `decay()` (0=fresh…1=faded from time since last exercised) +
      `last_recalled`/`decay` on backlinks/outgoing + `restore_edge`; `restore_link` command
      re-justifies (empty rejected → friction kept) then resets decay.
- [x] **#14 Spaced-repetition quizzing** — surface "how does A relate to B?" on a schedule from
      `last_recalled` / `recall_strength`.
      touches: `src-tauri/src/index.rs`, `recall.rs`, `commands.rs` · blocked-by: none
      (shares files with #13 → same lane, run after #13)
      Done: `index.rs` `review_interval_days`/`is_due` (SM-2 geometric backoff) + `due_reviews()`
      (most-overdue-first, justification withheld); `recall.rs` `grade_review` reveals the why
      only after a committed self-grade and records the rep; `due_reviews`/`grade_review` commands.
- [x] **#15 "What to review" surface** — list the connections most often failed, driven by
      `recall_log`.
      touches: `src-tauri/src/index.rs`, `commands.rs`, `src/routes/+page.svelte` · blocked-by: #13
      Done: `index.rs` `most_failed_connections` aggregates `recall_log` (most-failed first,
      endpoints only — justification withheld); `what_to_review` command + `api.ts`; sidebar
      "what to review" panel lists weak connections and routes you to re-recall them.
- [x] **#16 CodeMirror Markdown editor** — replaced the body `<textarea>` with CodeMirror +
      display-only `[[wiki-link]]` rendering. NO link autocomplete (see `CONTEXT.md`); linking
      still goes through the "Link from memory" flow. — `src/lib/Editor.svelte` (f024a2c)
      touches: `package.json`, `src/lib/Editor.svelte` (new), `src/routes/+page.svelte` (1 line)
      · blocked-by: none
- [x] **#17 Draw-the-edges graph** (39e9146) — user reconstructs the edges from memory on the
      `/graph` route, then scores them against the real `outgoing` edges (recalled/missed/spurious,
      with justifications revealed as feedback). Nodes shown; edges hidden; deterministic circle
      layout leaks no structure; nothing real drawn before scoring. The only graph the thesis allows.
      touches: `src/routes/graph/+page.svelte` (new), `src/lib/Graph.svelte` (new),
      `src/routes/+page.svelte` (1 line nav) · blocked-by: none
      (⚠ shares the 1-line `+page.svelte` edit with #16 — sequence those two touches)
### #18 Capture bundle (epic — split into the vertical slices below)

Each slice owns a NEW Rust module instead of bloating `vault.rs`/`index.rs`. The four shared
seams — `commands.rs`, `lib.rs` (the `generate_handler!` list), `src/lib/api.ts`, and
`src/routes/+page.svelte` (nav) — are **append-only**: add your entry, never reorder, rebase
before pushing. **Thesis guardrail (CONTEXT.md):** capture is allowed, but *connecting* a
captured note stays in the no-autocomplete justified-link flow — import/clip/templates create
notes, never edges; tag browsing is an escape hatch, not the default path.

- [x] **#18-wave0 Capture scaffold** — make the seams append-friendly once: a data-driven nav
      array in `+page.svelte`, a `capture` namespace in `api.ts`, the command-registration
      pattern in `lib.rs`. Gates #18c/#18d. (First commit of the capture-scaffold-notes lane.)
      touches: `src/routes/+page.svelte`, `src/lib/api.ts`, `src-tauri/src/lib.rs` · blocked-by: none
- [x] **#18a Note templates** — scaffolding for new-note bodies (NOT a capture shortcut: no
      autocomplete, links still justified).
      touches: `src-tauri/src/templates.rs` (new), `commands.rs`, `lib.rs`, `api.ts`, capture sub-route
      · blocked-by: #18-wave0
- [x] **#18b Daily/fleeting notes** — a dated quick-capture note; reuses the template logic.
      touches: `src-tauri/src/daily.rs` (new), `commands.rs`, `lib.rs`, `api.ts`, capture sub-route
      · blocked-by: #18a
- [x] **#18c Tags-as-navigation** — browse notes by tag behind a disclosure, modelled on the
      #11 search escape hatch (present but unglamorous; never the default path).
      touches: read-only tag query in `src-tauri/src/index.rs`, `commands.rs`, `lib.rs`, `api.ts`,
      tags panel/route · blocked-by: #18-wave0
- [x] **#18d BibTeX/citation import** — parse `.bib`/arXiv → create a paper note (refs + body),
      NOT its edges. Connecting it stays in the justified linker flow.
      touches: `src-tauri/src/bibtex.rs` (new), `commands.rs`, `lib.rs`, `api.ts`, import sub-route
      · blocked-by: #18-wave0
- [x] **#18e Web clipping** — clip a URL → a note (title + URL ref + extracted body), NOT its
      edges. Reuses #18d's paper-note helper. Same thesis guardrail as #18d.
      touches: `src-tauri/src/clip.rs` (new), `commands.rs`, `lib.rs`, `api.ts`, clip sub-route
      · blocked-by: #18d

Lanes: **A** capture-scaffold-notes (wave0 → #18a → #18b, opus) · **B** capture-import
(#18d → #18e, opus) · **C** tags-navigation (#18c, sonnet). B and C start once #18-wave0 is on `main`.

### #19+ The reading layer (see CONTEXT.md "The reading layer" — read freely, connect from memory)

- [x] **#19 Sources library** — drag-drop a PDF onto the window → it becomes a source note
      only after the user writes their own name + one-sentence idea (generation effect at
      ingest; no edges). Zotero-feel library view (newest first, idea snippet, tags), PDF
      opens in the system viewer; "Open PDF ↗" from the note, `/?note=<id>` deep link back.
      touches: `src-tauri/src/sources.rs` (new), `commands.rs`, `lib.rs`, `api.ts`,
      `src/routes/library/`, nav · blocked-by: none
**#20 Recall-gated link carousel** (split into the two slices below — same lane, serial:
both edit the notes-view link flow). Thesis guardrail: recall gates the carousel — cards are
face-down until the user types the exact title from memory; it must never become a readable
pick-list (CONTEXT.md ban on candidate lists), and flipping must reuse the exact resolver
(`resolve_link`), never a fuzzy match.

- [x] **#20a Tweet-constrained justification** — cap the edge `why` at 140 chars, enforced in
      `linker.rs::commit_edge` (reject over-cap with a clear error; tests) and surfaced in the
      UI as a live character counter + maxlength on the link/restore justification inputs.
      Compression is elaboration: a hard cap forces the essence of the relationship.
      touches: `src-tauri/src/linker.rs`, `src/routes/+page.svelte` (link panel only)
      · blocked-by: none
- [ ] **#20b Flip-to-recall carousel** — a strip of face-down cards (one per note, shuffled,
      titles hidden) shown in the link flow; typing a title from memory flips only its exact
      match (via existing `resolve_link` — NO new backend, NO candidate list); a flipped card
      can be dragged onto the open note to start the edge, landing in the #20a tweet-capped
      justify step. Flips may persist for the session (a scoreboard of what you've retrieved),
      but never across sessions. No new Tauri commands.
      touches: `src/lib/LinkCarousel.svelte` (new), `src/routes/+page.svelte` (link flow)
      · blocked-by: #20a
