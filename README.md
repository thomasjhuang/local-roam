# local-roam

A standalone, local-first macOS notebook for ML research — nimble, calm, plain text, in the
spirit of [The Archive](https://zettelkasten.de/the-archive/) — built on **atomic cards
assembled into threads**, with the source papers living inside it.

> **Pivot note (2026-07-01):** the original "productive friction" recall thesis (typed-from-
> memory links, quiz-before-reveal backlinks) is retired, and the positive direction is a
> card/thread Folgezettel model — see [`CONTEXT.md`](./CONTEXT.md) ("The core model").
> `docs/PRD.md` describes the pre-pivot v1 and is historical.

## The model

1. **Cards** — one atomic idea per file; no title needed (the first line is its label).
2. **Threads** — a manifest note (a nested list of card links) that reads top-to-bottom as
   one flowing long-form note. A card can belong to several threads.
3. **Folgezettel, derived** — every card shows an address (`2a1`) computed from its place in
   a thread; reorderable, so it never decays. New cards continue a thread, branch off the
   current card, or start a new one.
4. **A paper is a thread** — paste an arXiv link or drop a PDF and a paper thread appears
   with fetched metadata; you read it in a side pane and grow the thread card by card.

Your notes are plain Markdown files in a folder you choose; a SQLite index (a rebuildable
cache) lives in `.local-roam/` inside the vault. No lock-in — the vault stays usable in
The Archive, Obsidian, or `grep`.

## Develop

Requires Rust, Node, and the macOS toolchain.

```sh
npm install
npm run tauri dev     # run the app
```

## Test

```sh
cd src-tauri && cargo test    # Rust unit tests for the four deep modules
```

## Build a release app

```sh
npm run tauri build
```

## Layout

- `src-tauri/src/vault.rs` — Markdown files + frontmatter (source of truth)
- `src-tauri/src/index.rs` — SQLite mirror (rebuildable cache: nodes, links, search)
- `src-tauri/src/sources.rs`, `bibtex.rs`, `clip.rs` — capture + the document layer
- `src/routes/+page.svelte` — the UI
- `TASKS.md` — ordered issue tracker (v3 pivot roadmap at the bottom)
