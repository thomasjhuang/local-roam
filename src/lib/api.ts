import { invoke } from "@tauri-apps/api/core";

export interface NodeMeta {
  id: string;
  title: string;
  aliases: string[];
  refs: string[];
}

export interface Link {
  to: string;
  why: string;
}

export interface Note {
  id: string;
  title: string;
  created: string;
  aliases: string[];
  tags: string[];
  refs: string[];
  links: Link[];
  body: string;
}

export interface Backlink {
  from_id: string;
  from_title: string;
  why: string;
}

export interface OutLink {
  to_id: string;
  to_title: string;
  why: string;
}

/** A tag and how many notes carry it (#18c). For the tag-browsing escape hatch. */
export interface TagCount {
  tag: string;
  count: number;
}

/** A note that carries a local PDF (#19) — the library's literature-note tier. */
export interface SourceMeta {
  id: string;
  title: string;
  pdf_path: string;
  idea: string;
  tags: string[];
  created: string;
}

/** A built-in new-note body skeleton (#18a). Structure only — never edges. */
export interface Template {
  id: string;
  name: string;
  description: string;
  tags: string[];
  body: string;
}

// --- v3 card/thread model (#22) ---

/** A card: opaque id + first-line label (cards are untitled by default). */
export interface CardMeta {
  id: string;
  label: string;
  title: string | null;
}

/** A thread (a paper or an idea thread) with its card count. */
export interface ThreadMeta {
  id: string;
  title: string;
  refs: string[];
  card_count: number;
}

/** A card as it sits in one thread, with its derived Folgezettel address. */
export interface ThreadCard {
  card_id: string;
  address: string;
  label: string;
  position: number;
}

/** One thread a card belongs to, with the card's derived address there. */
export interface CardMembership {
  thread_id: string;
  thread_title: string;
  address: string;
}

export const api = {
  getSavedVault: () => invoke<string | null>("get_saved_vault"),
  openVault: (path: string) => invoke<void>("open_vault", { path }),
  listNotes: () => invoke<NodeMeta[]>("list_notes"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  createNote: (title: string, refs: string[], aliases: string[], tags: string[], body: string) =>
    invoke<Note>("create_note", { title, refs, aliases, tags, body }),
  saveNote: (id: string, title: string, body: string, refs: string[], aliases: string[], tags: string[]) =>
    invoke<Note>("save_note", { id, title, body, refs, aliases, tags }),
  deleteNote: (id: string) => invoke<void>("delete_note", { id }),
  /** Returns the matched note on exact recall, or null. Never a candidate list. */
  resolveLink: (attempt: string) => invoke<NodeMeta | null>("resolve_link", { attempt }),
  commitLink: (fromId: string, toId: string, justification: string) =>
    invoke<void>("commit_link", { fromId, toId, justification }),
  outgoing: (id: string) => invoke<OutLink[]>("outgoing", { id }),
  /** The notes that link to this one, shown directly (the recall gate is retired). */
  backlinks: (id: string) => invoke<Backlink[]>("backlinks", { id }),
  search: (query: string) => invoke<NodeMeta[]>("search", { query }),
  /**
   * Tag-browsing escape hatch (#18c). Navigation only — like `search` above, present
   * but never the default path; browsing a tag surfaces existing notes, never creating
   * a note or an edge.
   */
  listTags: () => invoke<TagCount[]>("list_tags"),
  notesByTag: (tag: string) => invoke<NodeMeta[]>("notes_by_tag", { tag }),
  /**
   * Sources library (#19) — the reading layer, frictionless by design: browsing
   * and opening a PDF trains nothing and gates nothing. The recall thesis governs
   * the connections around a source, not access to it.
   */
  listSources: () => invoke<SourceMeta[]>("list_sources"),
  /** Open the source's PDF in the system viewer (Preview). */
  openSource: (id: string) => invoke<void>("open_source", { id }),

  /**
   * v3 card/thread model (#22) — read surfaces for the new UI (#23+). The vault is the
   * source of truth; these expose the derived card/thread/membership cache. Folgezettel
   * addresses come back derived, never stored.
   */
  listThreads: () => invoke<ThreadMeta[]>("list_threads"),
  /** The cards of a thread in manifest order, each with its derived address. */
  threadCards: (threadId: string) => invoke<ThreadCard[]>("thread_cards", { threadId }),
  listCards: () => invoke<CardMeta[]>("list_cards"),
  /** Every thread a card belongs to — a card in two threads returns two addresses. */
  cardMemberships: (cardId: string) => invoke<CardMembership[]>("card_memberships", { cardId }),
  /** The ids a card links to (from its body wiki-links); a target may be a card or thread. */
  cardTargets: (cardId: string) => invoke<string[]>("card_targets", { cardId }),

  /**
   * Capture namespace — features that *create notes* (templates, daily notes, imports,
   * web clips). Thesis guardrail: capture creates notes, never edges. Connecting a
   * captured note still goes through the no-autocomplete justified-link flow above.
   * Each #18 slice appends its binding here — append-only, never reorder.
   */
  capture: {
    /** The built-in note-body templates (#18a). */
    listTemplates: () => invoke<Template[]>("list_templates"),
    /** Create a note from a template id; pre-fills the body skeleton, no edges. */
    createFromTemplate: (templateId: string, title: string) =>
      invoke<Note>("create_from_template", { templateId, title }),
    /** Open (creating once, if needed) today's dated quick-capture note (#18b). */
    openDailyNote: () => invoke<Note>("open_daily_note"),
    /**
     * Import a BibTeX entry (parsed offline) or an arXiv id/URL (fetched from arXiv)
     * into a paper note — refs + body skeleton, no edges (#18d).
     */
    importCitation: (input: string) => invoke<Note>("import_citation", { input }),
    /** Clip a URL → a note with its title, the URL as a ref, and the extracted body — no edges (#18e). */
    clipUrl: (url: string) => invoke<Note>("clip_url", { url }),
    /**
     * A dropped PDF → a source note (#19). The name and idea are the user's own
     * words — the generation-effect friction at ingest. No edges, ever.
     */
    importPdf: (path: string, name: string, idea: string) =>
      invoke<Note>("import_pdf_source", { path, name, idea }),
  },
};
