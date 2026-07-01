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
- [x] **#20b Flip-to-recall carousel** — a strip of face-down cards (one per note, shuffled,
      titles hidden) shown in the link flow; typing a title from memory flips only its exact
      match (via existing `resolve_link` — NO new backend, NO candidate list); a flipped card
      can be dragged onto the open note to start the edge, landing in the #20a tweet-capped
      justify step. Flips may persist for the session (a scoreboard of what you've retrieved),
      but never across sessions. No new Tauri commands.
      touches: `src/lib/LinkCarousel.svelte` (new), `src/routes/+page.svelte` (link flow)
      · blocked-by: #20a

## v3 — Card/thread Folgezettel pivot (2026-07-01)

**Direction change, decided by the owner over a design grill.** The productive-friction /
recall thesis behind #9–#10, #13–#15, #17 and #20 is retired (see `CONTEXT.md` "The
pivot"). The positive direction — settled turn by turn in the grill, superseding the brief
"Archive-clone-with-PDFs" framing — is a **card/thread Folgezettel** notebook: The
Archive's soul (nimble, calm, plain text, search-first, autocomplete welcome) with a
card/thread structure, a *derived* Folgezettel address per card, and source papers mapped
in as first-class threads. Read `CONTEXT.md` "The core model" before touching #22+ — the
disk format is load-bearing.

Design decisions locked in the grill (do not relitigate — see [[recall-thesis-retired]]
memory for the why):
- **Card = one file** (opaque stable id filename, no required title, first line is the
  label). **Thread = a manifest note** (title + a nested Markdown list of `[[card]]` links
  = a classical structure note); reading a thread concatenates its cards into prose.
- **Folgezettel address is derived**, never stored: a pure function of a card's position in
  a manifest's nested list (`2a1`). Reorderable, so always currently-true, never decaying.
  A card may sit in **many threads** (many-to-many) → many addresses. Chosen over literal
  ids-in-filenames, which lock one lineage and decay.
- **Placement gesture** (continue / branch-off-current / new-thread) at card creation is
  the one deliberate act — authoring, not a quiz.
- **A paper is a thread** (manifest carries pdf/citekey/authors); reading = growing it;
  citing a paper = a wiki-link to its thread shown as the citekey. Idea threads = same
  object without refs. One ontology.

- [x] **#21 Demolish the recall machinery** (d47b72b) — clean slate before the new model. Delete
      `recall.rs` (guess scoring, review grading), decay/SM-2/`recall_log` in `index.rs`,
      the `what_to_review`/`due_reviews`/`grade_review`/`restore_link` commands, the `/graph`
      reconstruction quiz, the flip-to-recall carousel, and the recall gate on backlinks.
      `linker.rs`: drop the justification requirement + 140-char cap (kept only as a stopgap
      resolver until #22/#23 replace linking). All tests green after the cut; `rebuild_from_vault`
      still works (recall columns dropped).
      touches: `src-tauri/src/recall.rs` (delete), `index.rs`, `linker.rs`, `commands.rs`,
      `lib.rs`, `api.ts`, `src/routes/+page.svelte`, `src/routes/graph/` (delete),
      `src/lib/Graph.svelte` (delete), `src/lib/LinkCarousel.svelte` (delete) · blocked-by: none

- [ ] **#22 Card/thread data model + derived Folgezettel + migration** — the foundational
      new layer; everything downstream depends on it. `vault.rs`: two note kinds — **card**
      (opaque id, optional title, body = one atom) and **thread manifest** (title, optional
      refs, body = nested Markdown list of `[[card]]` links); parse/write the manifest tree.
      Folgezettel addressing as a **pure function** (manifest tree position → `2a1`), unit-
      tested standalone, no addresses on disk. `index.rs`: cards, threads, membership (with
      derived address), card→target links, FTS — all rebuildable. **Migration**
      (`rebuild_from_vault` tolerates both): each legacy note → a single-card thread of the
      same title; its frontmatter `links` become card-body wiki-links `[[id]] — <why>` (whys
      survive as prose); paper/source notes (a pdf ref) → paper threads (refs → manifest).
      Tests: address derivation over trees, a card in two threads → two addresses, migration
      round-trip, rebuild idempotent.
      touches: `src-tauri/src/vault.rs`, `index.rs`, `folgezettel.rs` (new, the pure address
      fn), `commands.rs`, `lib.rs`, `api.ts` · blocked-by: #21

- [ ] **#23 Thread view + card editor** — the make-or-break UI. Read a thread top-to-bottom
      as flowing prose (cards concatenated) with card boundaries visible; write *through*
      cards; split a paragraph into a new card via the **placement gesture** (continue /
      branch-off-current / new-thread). The manifest is maintained silently as you write
      (append card links), and stays hand-editable. `[[...]]` autocompletes over thread
      titles; link a specific card via copy-link/drag → `[[card-id]]`, rendered as the card's
      first line. Backlinks show the linking card's first line as context.
      touches: `src/lib/Editor.svelte`, `src/routes/+page.svelte`, `src-tauri/src/vault.rs`
      (manifest writes), `commands.rs`, `api.ts` · blocked-by: #22

- [ ] **#24 Collate & export** — walk a thread manifest in address order, concatenate its
      cards into one Markdown file (a paper thread → citation header + cards = a
      self-contained literature note). The "export a thread" action.
      touches: `src-tauri/src/export.rs` (new), `commands.rs`, `lib.rs`, `api.ts`,
      `src/routes/+page.svelte` · blocked-by: #22 (parallel with #23)

- [ ] **#25 Omnibar** — The Archive's core loop over the new model: one field; typing
      searches cards (first-line label + thread breadcrumb) and thread titles as you type;
      Return opens the top hit or creates. The sidebar list = threads (papers + idea threads),
      newest first, when the field is empty. Retires the separate search disclosure and the
      new-note mini-form.
      touches: `src/routes/+page.svelte`, `src-tauri/src/index.rs` (ranking), `commands.rs`,
      `lib.rs`, `api.ts` · blocked-by: #22

- [ ] **#26 Paper threads (the arXiv layer)** — paste an arXiv URL/id or drop a PDF → a
      **paper thread** with fetched title/authors/abstract/citekey in the manifest, PDF filed
      to `<vault>/literature/`. Reading grows the thread's cards; citing a paper elsewhere =
      `[[paper-thread]]` shown as the citekey; maintain `<vault>/literature/literature.bib`
      from paper-thread refs. Folds `sources.rs` + `bibtex.rs` into the thread model; no
      naming/idea gate (retired).
      touches: `src-tauri/src/sources.rs`, `bibtex.rs`, `index.rs` (citekey lookup),
      `commands.rs`, `lib.rs`, `api.ts`, `src/routes/library/` · blocked-by: #22
      (own lane, parallel with #23/#25)

- [ ] **#27 Paper pane** — the mapped PDF one keystroke away: ⌘O opens it; a toggleable
      split pane renders the PDF beside the thread view so you read the paper and write cards
      into its thread side by side; an arXiv ref without a local PDF offers fetch-and-file.
      touches: `src/lib/PdfPane.svelte` (new), `src/routes/+page.svelte`,
      `src-tauri/src/sources.rs` (fetch/open), `commands.rs`, `lib.rs`, `api.ts`
      · blocked-by: #23, #26

- [ ] **#28 The two-level graph** — the only graph the new thesis allows: draws **only
      hand-made** connections (no similarity, ever). Top level: threads as nodes (sized by
      card count); edges of two manual kinds — *link edges* (a card in A links into B) and
      *membership edges* (a card in both manifests); hover lists the linking cards. Drill into
      a node → the thread's **Folgezettel spine** (trunk + branches, cross-thread links as
      ports). Read-only navigation first.
      touches: `src/routes/graph/` (new), `src/lib/Graph.svelte` (new),
      `src-tauri/src/index.rs` (thread-edge aggregation), `commands.rs`, `api.ts`
      · blocked-by: #22 (ideally after #23)

- [ ] **#29 Polish** — clickable `#hashtags` run an omnibar search; saved searches pinned in
      the sidebar; typewriter-mode toggle; optional global thread index / section numbering
      (Doto's leading-digit "sections") if wanted.
      touches: `src/lib/Editor.svelte`, `src/routes/+page.svelte`, `settings.rs`
      · blocked-by: #25
