# local-roam

A standalone, local-first macOS notebook for understanding how ML research papers and ideas
connect — by making **you** remember the connections, not the tool.

It's an org-roam / Zettelkasten descendant with an inverted goal: instead of frictionless
capture and pretty auto-graphs, it uses *productive friction* (recall, justification) so the
act of using it strengthens what you know. See [`CONTEXT.md`](./CONTEXT.md) for the design
thesis and [`docs/PRD.md`](./docs/PRD.md) for the full spec.

## The two core mechanics (MVP)

1. **No-autocomplete, justified linking** — to link to another note you type its title from
   memory (no dropdown), and you must write one sentence saying *why* they connect.
2. **Recall-before-reveal backlinks** — opening a note first asks which notes you think link to
   it, scores your recall, then reveals the real backlinks as feedback.

Your notes are plain Markdown files in a folder you choose; a SQLite index (a rebuildable
cache) lives in `.local-roam/` inside the vault.

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
- `src-tauri/src/index.rs` — SQLite mirror + recall telemetry
- `src-tauri/src/linker.rs` — mechanic #1 (link from memory)
- `src-tauri/src/recall.rs` — mechanic #2 (recall before reveal)
- `src/routes/+page.svelte` — the UI
- `TASKS.md` — ordered issue tracker (MVP + v2 backlog)
