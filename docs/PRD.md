# PRD: local-roam — a forced-memory research notebook

> Working name: **local-roam**. A standalone, local-first macOS app for one user (the
> author) to build and *internalize* the connections between ML research papers and ideas.
> Unlike org-roam / Obsidian, which optimize for frictionless capture and retrieval, this
> tool deliberately introduces *productive friction* so the user — not the tool — does the
> remembering.

## Problem Statement

As an ML researcher, I read a lot of papers and form a lot of ideas, but the tools I use to
organize them (Obsidian, etc.) do the remembering *for* me. Autocomplete inserts links I
never have to recall, the graph hands me a map I never had to build, and backlinks appear
without my ever retrieving them. The result is a beautiful, browsable knowledge base that I
don't actually *know*. I can find how two papers relate by clicking — but I can't recall it,
and recall is what matters when I'm doing research and need the connections in my head.

I want the opposite: a tool that forces me to reconstruct the structure of my research
knowledge from memory, so that the act of using it strengthens what I know rather than
outsourcing it.

## Solution

A standalone desktop app that owns a vault of plain-text Markdown notes (one per paper or
idea) and an auxiliary index of the links between them. Every place where a conventional
tool would do the remembering for me, this app instead makes me retrieve from memory first
and only *then* reveals the answer as feedback. The friction is productive — grounded in
desirable-difficulty, generation-effect, and retrieval-practice research — not arbitrary
slowness.

v1 delivers a complete loop built on two mechanics:

1. **No-autocomplete, justified linking.** To link the current note to another, I type the
   target note's title *from memory* — no dropdown, no fuzzy-match. A link cannot be created
   without also writing a one-sentence justification of *why* the two connect. Empty edges
   are forbidden.
2. **Recall-before-reveal backlinks.** Opening a note does not show what links to it. The app
   first asks "what connects here?"; I type my guesses; it scores them against the real
   backlinks; then it reveals them as feedback. Every revisit is a retrieval rep.

## User Stories

1. As a researcher, I want to pick a local folder as my vault, so that my notes are plain
   files I own and can back up or version with git.
2. As a researcher, I want to create a note for an idea, so that I can capture a concept I'm
   thinking about.
3. As a researcher, I want to create a note for a paper and attach its arXiv id / DOI /
   citation key as a `ref`, so that the note is anchored to a real source.
4. As a researcher, I want every note to have a stable unique id independent of its title, so
   that I can rename notes without breaking links.
5. As a researcher, I want to give a note aliases, so that I can recall it under more than one
   name (e.g. "AdamW" / "decoupled weight decay").
6. As a researcher, I want to edit a note's body in Markdown, so that I can write freely.
7. As a researcher, when I want to link to another note, I want to type its title from memory
   with no autocomplete dropdown, so that I am forced to retrieve it rather than recognize it.
8. As a researcher, when my recalled title matches an existing note, I want the link created,
   so that correct recall is rewarded.
9. As a researcher, when my recalled title does NOT match, I want to be told it didn't resolve
   (without being handed the answer immediately), so that I get another retrieval attempt.
10. As a researcher, after a failed attempt, I want the option to deliberately create a new
    note instead, so that I can consciously decide "this really is new" vs "I forgot the
    existing one".
11. As a researcher, I want to be required to write one sentence justifying every link I
    create, so that no connection exists in my vault without an understood reason.
12. As a researcher, I want to see and edit the justification on an existing edge, so that I
    can refine my understanding over time.
13. As a researcher, when I open a note, I want the app to first ask me which notes link to it
    before showing me, so that I practice retrieving my own structure.
14. As a researcher, I want to type my backlink guesses and have them scored against reality,
    so that I get immediate feedback on how well I know the connections.
15. As a researcher, after guessing, I want the true backlinks revealed (with their
    justifications), so that I learn what I missed.
16. As a researcher, I want each backlink revealed with the justification I wrote for it, so
    that the reason for the connection is reinforced on every revisit.
17. As a researcher, I want my recall attempts (when and how well I recalled an edge) recorded,
    so that later features (spaced repetition, link decay) can build on that history.
18. As a researcher, I want to search my notes by title/alias/content, so that I can still find
    a note when I genuinely cannot recall it (an escape hatch, used sparingly).
19. As a researcher, I want the index rebuildable from the Markdown files at any time, so that
    if the index is lost or corrupted nothing of value is lost.
20. As a researcher, I want the app to work fully offline with no account, so that my research
    notes stay private and local.
21. As a researcher, I want to see a note's outgoing links (the ones I authored), so that I can
    review what I connected it to.
22. As a researcher, I want to delete a note and have its edges cleaned up, so that the vault
    stays consistent.
23. As a researcher, I want to rename a note's title without breaking existing links, so that
    titles can evolve as my understanding does.

## Implementation Decisions

**Stack:** Tauri 2 — Rust core (vault file I/O, SQLite index, file-watching) + a lightweight
web frontend (Svelte + Vite) for the custom friction UI. Editor via CodeMirror. Chosen over
SwiftUI because the friction interactions (typed-from-memory prompts, quiz-then-reveal flows,
and the v2 draw-the-edges graph) are far cheaper to build with the web ecosystem, while Tauri
still produces a small native `.app`.

**File format:** each note is one Markdown file with YAML frontmatter:
`id` (uuid), `title`, `created`, `aliases: []`, `tags: []`, `refs: []` (arXiv/DOI/citation
key). Body is Markdown. **Papers and ideas are the same entity ("note"); a paper is just a
note that carries a `ref`.** (Open decision, defaulted to uniform — vetoable.)

**Edges are first-class.** A link is not merely inline Markdown; it is a stored edge:
`{from_id, to_id, justification, created, last_recalled, recall_strength}`. The
`last_recalled` / `recall_strength` columns are populated trivially in v1 but exist now
because v2 (spaced repetition, link decay) depends on them — adding them later would be a
migration.

**Modules (aiming for deep, isolated, testable units):**

- **Vault** — owns the on-disk Markdown files and frontmatter (de)serialization. Interface:
  `list_notes`, `read_note`, `write_note`, `create_note`, `delete_note`, `rename_title`. The
  only module that knows the file format.
- **Index** — the SQLite mirror. Interface: `rebuild_from_vault`, `nodes`, `backlinks(id)`,
  `outgoing(id)`, `upsert_edge`, `delete_edges_for(id)`, `record_recall(edge, score)`,
  `search(query)`. Always reconstructable from Vault.
- **Linker** — the friction core for mechanic #1. Pure resolution logic: `resolve(attempt)
  -> Exact(id) | NoMatch`, and `commit_edge(from, to, justification)` which *rejects* an empty
  justification. Deliberately does NOT expose prefix/fuzzy candidate lists during entry.
- **RecallSession** — the friction core for mechanic #2. `start(note_id)` (knows the true
  backlinks but hides them), `submit_guesses([title])` -> per-guess scored result +
  full reveal, and records recall reps via Index.
- **Tauri command layer** — thin glue exposing the above to the Svelte frontend; intentionally
  shallow, not unit-tested.

## Testing Decisions

Good tests here verify **external behavior, not implementation**: given inputs to a module's
public interface, assert outputs/observable state — never reach into private structure. Tests
should survive a rewrite of a module's internals.

Modules to test (the four deep modules; the Tauri glue and UI are out of scope for unit tests
in v1):

- **Vault** — round-trip: a note written then read back yields identical frontmatter + body;
  `rename_title` preserves `id`; `create_note` produces a valid parseable file. Prior art:
  standard serializer round-trip tests.
- **Index** — against a known fixture vault, `backlinks`/`outgoing`/`search` return the correct
  sets; `rebuild_from_vault` is idempotent and reproduces the same graph; `record_recall`
  updates `last_recalled`/`recall_strength`. Run against an in-memory SQLite db.
- **Linker** — exact recalled title resolves; near-miss / wrong title returns `NoMatch` (does
  *not* leak the intended target); `commit_edge` rejects empty/whitespace justification and
  accepts a real one. This is the highest-value test target — it encodes the core thesis.
- **RecallSession** — guesses are scored correctly against true backlinks (hits, misses,
  spurious guesses); the reveal contains exactly the real backlinks with justifications; a
  recall rep is recorded.

## Out of Scope (v1)

- Graph view of any kind — including the v2 *draw-the-edges-from-memory* reconstruction.
- Spaced-repetition quizzing of relationships and **link decay** (the columns exist; the
  behavior does not).
- Daily/fleeting notes, templates, tags-as-navigation, citation import/BibTeX, web clipping.
- Sync, accounts, multi-vault, mobile, Windows/Linux, any packaging for distribution.
- Rich PDF/paper ingestion or annotation.

## Further Notes

- Design principle to hold the line on: *anywhere a conventional tool would remember for the
  user (autocomplete, instant backlinks, auto-graph), this app makes the user reproduce it
  from memory first and reveals the answer only as feedback.* Friction must be productive
  (forces encoding), never arbitrary (a loading spinner teaches nothing).
- The search escape hatch (#18) is a deliberate tension: it must exist (so a forgotten note is
  not permanently lost) but should feel like a last resort, not a default. v1 keeps it present
  but unglamorous.
- No issue tracker / triage vocabulary was configured for this skill, so this PRD lives in the
  repo. It can be published to Linear on request (needs team/project confirmation first).
