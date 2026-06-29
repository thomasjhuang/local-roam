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
  recall_strength: number;
}

export interface OutLink {
  to_id: string;
  to_title: string;
  why: string;
}

export interface RecallResult {
  hits: Backlink[];
  missed: Backlink[];
  spurious: string[];
  reveal: Backlink[];
}

/** A connection the user keeps failing to recall — the justification is withheld. */
export interface FailedConnection {
  from_id: string;
  from_title: string;
  to_id: string;
  to_title: string;
  failures: number;
  attempts: number;
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
  submitRecall: (noteId: string, guesses: string[]) =>
    invoke<RecallResult>("submit_recall", { noteId, guesses }),
  search: (query: string) => invoke<NodeMeta[]>("search", { query }),
  /** Connections most often failed in recall, most-failed first. Justification withheld. */
  whatToReview: (limit: number) => invoke<FailedConnection[]>("what_to_review", { limit }),

  /**
   * Capture namespace — features that *create notes* (templates, daily notes, imports,
   * web clips). Thesis guardrail: capture creates notes, never edges. Connecting a
   * captured note still goes through the no-autocomplete justified-link flow above.
   * Each #18 slice appends its binding here — append-only, never reorder.
   */
  capture: {},
};
