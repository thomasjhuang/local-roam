# CONTEXT — local-roam

> Read this before changing anything. **Rewritten 2026-07-01** across two moves: a pivot
> that retired the old "productive friction / recall" thesis, then a design grill that
> settled what replaces it — a **card-and-thread Folgezettel** notebook with the papers
> living inside it. If you remember recall quizzes, or a plain "Archive clone with PDFs",
> both are superseded by what's below.

## What this is

A standalone, local-first macOS notebook for **one user** to think and write about ML
research. It takes The Archive's *soul* — nimble, calm, plain text, search-first, no
lock-in — and adds the thing The Archive doesn't have: a **card/thread structure** for
atomic ideas, a **derived Folgezettel** address for every card, and the **source papers
mapped in** as first-class threads you read and annotate side by side.

The one-liner: **atomic cards, assembled into readable threads, with the paper right there.**

## The pivot (2026-07-01) — read this if you knew the old app

v1/v2 (#1–#20) was built on an inverted thesis: the tool refused to remember for you —
no autocomplete, links typed from memory, backlinks hidden behind recall quizzes, spaced
repetition, a flip-to-recall carousel. **All retired.** The owner's verdict: recall gates
were friction without joy, Anki bolted onto a notebook.

Retired, remove on sight, never reintroduce: recall-before-reveal backlinks, guess
scoring, recall telemetry, "what to review", spaced repetition, link decay,
re-justify-to-restore, exact-or-nothing resolution, the no-autocomplete rule, mandatory
per-edge justifications and the 140-char cap, the draw-the-edges graph quiz, the
flip-to-recall carousel.

**Standing ban:** no mechanic that quizzes the user or gates information behind recall.
Autocomplete, instant backlinks, and fuzzy search are now *desirable*, not forbidden.

## The thesis (new)

**Nimble, calm, plain text — atomic cards assembled into threads, the paper right there.**

Grounded not in learning-science friction but in the Zettelkasten community's own
conclusion (see the design grill and the Folgezettel debate): the box is a **thinking
partner**; it remembers so you can think. Atomicity and sequence come from Luhmann, but
freed of the paper medium's constraints — the digital medium removes card-size limits and
single-lineage filing, so we keep what atomicity buys (free recombination of ideas) and
drop what the cards imposed (forced splitting, one-parent-only, decaying implicit order).

## The core model (this is the load-bearing part)

Everything is plain Markdown files in a vault folder. Two kinds of note:

1. **Card** — one file, one atomic idea. The filename is an **opaque stable id**
   (never changes); the card needs **no title** — its *first line* is its display label
   everywhere (search, backlinks, graph). Frontmatter is minimal (`id`, `created`,
   `tags`). Cards are found by search, by their thread, or by following links — never by
   remembering a name.

2. **Thread** — one file, a **manifest** (a classical *structure note*). It has a title,
   and its body is a **nested Markdown list of card links** that defines an ordered tree:

   ```
   - [[card-a]]        → address 1
   - [[card-b]]        → address 2
     - [[card-c]]      → address 2a      (branches off 2)
       - [[card-d]]    → address 2a1
     - [[card-e]]      → address 2b
   - [[card-f]]        → address 3
   ```

   The thread *is* the readable long-form note: reading it top-to-bottom concatenates its
   cards into flowing prose; the card boundaries are where ideas can branch off to other
   threads. The manifest is maintained **silently** as you write a thread, but is plain
   text you *can* hand-edit or open in any Markdown tool.

**Folgezettel addresses are derived, never stored.** A card's address (`2a1`) is a pure
function of its position in a thread's nested list — trunk items number `1, 2, 3`; each
nesting level alternates letters/numbers (`2a`, `2a1`, `2a1a`). Because the manifest is
explicit and reorderable, an address is always *currently true*, not a historical
artifact that decays. **A card may belong to more than one thread** and therefore has more
than one address — the many-to-many that Luhmann's paper filing could not express. This is
Folgezettel-as-derived-address, chosen over literal-id-in-filename precisely because the
literal form locks a card to one lineage and decays (see the forum debate in the grill).

The **placement gesture** replaces the old recall friction as the one deliberate act at
card creation: a new card is born either **continuing** the current thread, **branching
off** the card you're on, or **starting a new thread**. One keystroke; it is *authoring*,
not a quiz.

**A paper is a thread.** Importing an arXiv id / dropping a PDF creates a thread whose
manifest carries the metadata (title, authors, citekey, PDF path). Reading a paper =
*growing that thread*: each idea you lift becomes a card in its sequence, and can
cross-link to cards in other threads. **Citing** a paper elsewhere is just a wiki-link to
its thread, rendered as the citekey — no separate citation syntax. Collating a paper
thread yields a self-contained literature note (citation header + your cards in order).

**Idea threads** are the same object without refs. Papers and thoughts are one ontology.

## Architecture (deep modules)

The vault (Markdown files) is the **single source of truth**. The SQLite index is a
rebuildable cache; delete it and nothing of value is lost.

- `vault.rs` — the only module that knows the file format: card files and thread manifests,
  their (de)serialization, and manifest tree parsing/writing.
- `index.rs` — SQLite mirror: cards, threads, thread-membership (with **derived** address),
  card→target links, full-text search. `rebuild_from_vault` reconstructs all of it.
- Folgezettel addressing lives as a **pure function** over a manifest tree (unit-tested in
  isolation) — position in → address out. No addresses on disk.
- `sources.rs` / `bibtex.rs` / `clip.rs` / `daily.rs` / `templates.rs` — capture; papers,
  daily threads, imports. All create cards/threads, never edges.
- `commands.rs` / `state.rs` / `settings.rs` — thin Tauri glue (not unit-tested).

## The graph (the only graph the new thesis allows)

Two levels, and it **only draws connections the user made by hand** — no similarity, no
suggestions, ever (that ban survives both eras). Top level: **threads as nodes** (sized by
card count), edges of two manual kinds — *link edges* (a card in A links into B) and
*membership edges* (a card sits in both manifests). Drill into a node → the thread's
**Folgezettel spine** (trunk + branches), with cross-thread links shown as ports leaving
the frame. Read-only navigation first; editing-in-the-graph is a later maybe.

## Things that look like bugs but are features

- No cloud, no accounts, no sync engine — the vault is a folder; sync is your cloud drive
  or git. **Intentional.**
- Cards have no titles. **Intentional** — naming every atom is the ceremony that kills
  card-by-card capture; only threads (the things you return to by name) are titled.
- The index is deletable at any time and rebuilds from files. **Intentional.**
- Feature restraint: The Archive's makers call extra features "distractions". When in
  doubt, leave it out.

## Roadmap

See `TASKS.md` v3 section: #21 demolishes the recall machinery; #22 lays the card/thread
data model + derived Folgezettel + migration; #23 the thread view / card editor; #24
collate-and-export; #25 the omnibar; #26 paper threads (the arXiv layer); #27 the paper
pane; #28 the two-level graph; #29 polish.
