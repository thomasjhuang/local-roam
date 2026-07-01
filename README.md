# local-roam

A standalone, local-first macOS Zettelkasten for ML research — nimble, calm, plain text,
in the spirit of [The Archive](https://zettelkasten.de/the-archive/) — with the papers
living inside it: every note can map to an arXiv id / DOI / PDF that opens in one keystroke.

> **Pivot note (2026-07-01):** the original "productive friction" recall thesis (typed-from-
> memory links, quiz-before-reveal backlinks) is retired — see the pivot section in
> [`CONTEXT.md`](./CONTEXT.md). `docs/PRD.md` describes the pre-pivot v1 and is historical.

## The core loop

1. **Omnibar** — one field: type to search everything; Return opens the match or creates
   the note. The note list is the result list.
2. **Wiki-links in prose** — `[[links]]` autocomplete as you type; backlinks are computed
   and shown instantly, with the sentence around the link as context.
3. **The paper is right there** — paste an arXiv link or drop a PDF and a paper note is
   created with fetched metadata; the document stays one keystroke away while you write.

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
