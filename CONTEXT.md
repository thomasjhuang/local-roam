# CONTEXT — local-roam

> Read this before changing anything. **This file was rewritten on 2026-07-01 in a
> deliberate pivot** — if you remember an older thesis about "productive friction" and
> recall gates, that thesis is retired. Do not rebuild it.

## What this is

A standalone, local-first macOS Zettelkasten for **one user** to think and write about ML
research — modeled closely on [The Archive](https://zettelkasten.de/the-archive/) (nimble,
calm, plain text), with one differentiator The Archive doesn't have: **the papers live
inside it.** Every note can be mapped to a document (arXiv id, DOI, local PDF), and the
mapped document is always one keystroke away.

## The pivot (2026-07-01) — read this if you knew the old app

v1/v2 (issues #1–#20) was built on an inverted thesis: the tool refused to remember for
you — no autocomplete, links typed from memory, backlinks hidden behind recall quizzes,
spaced repetition over edges, a flip-to-recall carousel. **All of that is retired.** The
owner's verdict after living with it: the recall gates were friction without joy — Anki
bolted onto a notebook. The Zettelkasten orthodoxy was right all along: the box is a
*thinking partner*; it remembers so you can think.

Concretely retired (remove on sight, never reintroduce):

- Recall-before-reveal backlinks, guess scoring, recall telemetry, "what to review".
- Spaced repetition, link decay, re-justify-to-restore.
- Exact-or-nothing link resolution and the no-autocomplete rule.
- Mandatory per-edge justifications and the 140-char cap.
- The draw-the-edges graph quiz and the flip-to-recall card carousel.

**Banned going forward: any mechanic that quizzes the user, gates information behind
recall, or adds friction in the name of learning.** If a feature makes the user prove
they remember something before showing it, it is wrong for this app.

## The thesis (new)

**Nimble, calm, plain text — and the paper is right there.**

1. **The tool remembers so you can think.** Frictionless capture, instant navigation,
   autocomplete everywhere it helps. Insight comes from writing in your own words and
   connecting ideas, not from being quizzed.
2. **Search is the interface.** One omnibar: type to search; press Return to open the
   match or create the note. The note list *is* the result list (The Archive's core loop).
3. **Plain text is the only truth.** One note = one Markdown file. The SQLite index is a
   rebuildable cache, nothing more. No lock-in, no proprietary anything; the vault must
   remain usable in The Archive, Obsidian, or `grep`.
4. **Links live in prose.** Connections are `[[wiki-links]]` written in the body, where
   the surrounding sentence naturally says *why* (link context the Zettelkasten way —
   encouraged by convention, never enforced by a validator). Backlinks are computed and
   shown instantly.
5. **Read freely.** A paper note opens its PDF in one keystroke; capture of an arXiv id
   or dropped PDF auto-fetches metadata (title, authors, abstract, citekey). Nothing
   about reading or capturing ever asks the user to earn it.

## Architecture (deep modules — unchanged in spirit)

- `vault.rs` — the only module that knows the file format. One note = one `<id>.md` with
  YAML frontmatter (`id`, `title`, `created`, `aliases`, `tags`, `refs`). The `id` never
  changes; titles rename freely.
- `index.rs` — SQLite mirror: nodes, body-derived links, backlinks, full-text search.
  `rebuild_from_vault` restores everything; deleting the index loses nothing.
- `sources.rs` / `bibtex.rs` / `clip.rs` / `daily.rs` / `templates.rs` — capture and the
  document layer (the arXiv differentiator).
- `commands.rs` / `state.rs` / `settings.rs` — thin Tauri glue (not unit-tested).

## Things that look like bugs but are features

- No cloud, no accounts, no sync engine — the vault is a folder; sync is the user's
  cloud drive or git. **Intentional.**
- The index can be deleted at any time. **Intentional** — it must always be rebuildable.
- Feature restraint: The Archive's owners call extra features "distractions". When in
  doubt, leave it out.

## Roadmap

See `TASKS.md` v3 section: #21 removes the recall machinery, #22 the omnibar, #23 body
wiki-links as the linking model, #24 the arXiv document layer, #25 Archive polish.
