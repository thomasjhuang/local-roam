# CONTEXT — local-roam

> Read this before changing anything. The whole point of this app is unusual, and the
> easy "improvement" is almost always the wrong one.

## What this is

A standalone, local-first macOS notebook for **one user** to deeply internalize how ML
research papers and ideas connect. It is an org-roam / Zettelkasten descendant, but with an
inverted goal.

## The thesis (do not violate this)

Every mainstream knowledge tool — Obsidian, Roam, org-roam — optimizes for *frictionless*
capture and retrieval. The tool remembers, so the user doesn't have to. The result is a
browsable database the user doesn't actually *know*.

local-roam optimizes for the opposite: **the tool refuses to remember for you, so your brain
has to.** The friction is the product. It is grounded in learning science — desirable
difficulty, the generation effect, retrieval practice, elaborative interrogation — not in
arbitrary slowness.

**Design rule:** anywhere a conventional tool would do the remembering for the user
(autocomplete, instant backlinks, an auto-drawn graph), local-roam instead makes the user
*reproduce it from memory first*, and reveals the answer only afterward as feedback.

Friction must be **productive** (forces encoding). A loading spinner teaches nothing and is
banned. Typing a paper's title from memory teaches a lot and is the whole idea.

## The two MVP mechanics

1. **No-autocomplete, justified linking** (`linker.rs`). To link to another note you type its
   title from memory — `resolve()` returns an exact match or nothing, and *never* a candidate
   list. `commit_edge()` refuses an empty justification: no edge exists without a reason.
2. **Recall-before-reveal backlinks** (`recall.rs`). Opening a note hides its backlinks;
   `submit_guesses()` scores what you recalled, records a rep for every true backlink
   (hit = success, miss = failure — both logged), then reveals.

## Architecture (deep modules)

The vault (Markdown files) is the **single source of truth**. The SQLite index is a
rebuildable cache + the home of recall *telemetry* only.

- `vault.rs` — the only module that knows the file format. One note = one `<id>.md` file with
  YAML frontmatter. The `id` never changes, so titles rename freely. Justified edges live in
  the source note's frontmatter `links` list.
- `index.rs` — SQLite mirror. `rebuild_from_vault` syncs without wiping recall telemetry.
- `linker.rs` — mechanic #1.
- `recall.rs` — mechanic #2.
- `commands.rs` / `state.rs` / `settings.rs` — thin Tauri glue (not unit-tested).

Knowledge is in the files; if the index is deleted, only usage stats are lost.

## Things that look like bugs but are features

- No fuzzy match / no autocomplete when linking. **Intentional.**
- Backlinks are hidden until you guess. **Intentional.**
- Full-text search exists but is buried under "escape hatch". **Intentional** — it must exist
  (so a forgotten note isn't lost forever) but must not become the default path.

## Roadmap

See `TASKS.md`. v2 builds on the recall telemetry already in the schema: draw-the-edges graph
reconstruction, spaced-repetition over relationships, and link decay.
