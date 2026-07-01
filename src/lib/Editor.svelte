<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    EditorView,
    keymap,
    placeholder as cmPlaceholder,
    Decoration,
    ViewPlugin,
    WidgetType,
    type DecorationSet,
    type ViewUpdate,
  } from "@codemirror/view";
  import { RangeSetBuilder, StateEffect } from "@codemirror/state";
  import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
  import { markdown } from "@codemirror/lang-markdown";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { autocompletion, completionKeymap, type CompletionContext } from "@codemirror/autocomplete";
  import { tags as t } from "@lezer/highlight";

  /** A `[[...]]` autocomplete candidate: a thread title or a card's first-line label. */
  export interface LinkCandidate {
    id: string;
    label: string;
    kind: "thread" | "card";
  }

  interface Props {
    value: string;
    placeholder?: string;
    /** Focus this editor on mount (a freshly-born card lands with the caret in it). */
    autofocus?: boolean;
    /** Resolve a `[[id]]` to its display label (thread title / card first line). */
    resolveLabel?: (id: string) => string | null;
    /** The `[[...]]` autocomplete candidates (thread titles + card labels). */
    candidates?: () => LinkCandidate[];
    /** Bumping this re-renders the link pills after threads/cards load. */
    linkVersion?: number;
    /** Persist this card (called on blur — writing *through* the card). */
    onSave?: () => void;
    /** ⌘⏎ split at the caret: head stays, tail becomes a new card. */
    onSplit?: (head: string, tail: string) => void;
    /** ArrowUp on the first line — move to the previous card. */
    onArrowUpOut?: () => void;
    /** ArrowDown on the last line — move to the next card. */
    onArrowDownOut?: () => void;
    /** Backspace at the very start — merge this card up into the previous one. */
    onMergeUp?: () => void;
    /** Click a `[[id]]` pill — open that thread/card. */
    onNavigate?: (id: string) => void;
  }

  let {
    value = $bindable(),
    placeholder = "",
    autofocus = false,
    resolveLabel,
    candidates,
    linkVersion = 0,
    onSave,
    onSplit,
    onArrowUpOut,
    onArrowDownOut,
    onMergeUp,
    onNavigate,
  }: Props = $props();

  let el: HTMLDivElement;
  let view: EditorView | undefined;

  // The CodeMirror extensions capture closures once; this ref lets them read the latest
  // callbacks/props on every keystroke without reconfiguring the editor. Populated (and
  // kept current) by the $effect below — never read the props directly here.
  const ctx: {
    resolveLabel?: Props["resolveLabel"];
    candidates?: Props["candidates"];
    onSave?: Props["onSave"];
    onSplit?: Props["onSplit"];
    onArrowUpOut?: Props["onArrowUpOut"];
    onArrowDownOut?: Props["onArrowDownOut"];
    onMergeUp?: Props["onMergeUp"];
    onNavigate?: Props["onNavigate"];
  } = {};
  $effect(() => {
    ctx.resolveLabel = resolveLabel;
    ctx.candidates = candidates;
    ctx.onSave = onSave;
    ctx.onSplit = onSplit;
    ctx.onArrowUpOut = onArrowUpOut;
    ctx.onArrowDownOut = onArrowDownOut;
    ctx.onMergeUp = onMergeUp;
    ctx.onNavigate = onNavigate;
  });

  /** Force a link-pill rebuild when the resolver's data (threads/cards) changes. */
  const refreshLinks = StateEffect.define<null>();
  $effect(() => {
    linkVersion; // track
    view?.dispatch({ effects: refreshLinks.of(null) });
  });

  // --- [[id]] rendered as its target's first line (a clickable pill) ---
  // Card and thread ids are opaque (uuids you never type), so we always replace the raw
  // `[[id]]` with the resolved label. This is display + navigation only — it never
  // suggests or infers a link; a pill exists solely because the user wrote that link.
  class LinkWidget extends WidgetType {
    id: string;
    label: string;
    constructor(id: string, label: string) {
      super();
      this.id = id;
      this.label = label;
    }
    eq(other: LinkWidget) {
      return other.id === this.id && other.label === this.label;
    }
    toDOM() {
      const span = document.createElement("span");
      span.className = "cm-linkpill";
      span.textContent = this.label;
      span.title = this.label;
      span.onmousedown = (e) => {
        e.preventDefault();
        ctx.onNavigate?.(this.id);
      };
      return span;
    }
    ignoreEvent() {
      return true;
    }
  }

  function buildLinks(view: EditorView): DecorationSet {
    const builder = new RangeSetBuilder<Decoration>();
    const re = /\[\[([^\[\]\n]+)\]\]/g;
    for (const { from, to } of view.visibleRanges) {
      const text = view.state.sliceDoc(from, to);
      let m: RegExpExecArray | null;
      while ((m = re.exec(text))) {
        const start = from + m.index;
        const end = start + m[0].length;
        const id = m[1].trim();
        const label = ctx.resolveLabel?.(id) ?? id;
        builder.add(start, end, Decoration.replace({ widget: new LinkWidget(id, label) }));
      }
    }
    return builder.finish();
  }

  const linkPills = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(v: EditorView) {
        this.decorations = buildLinks(v);
      }
      update(u: ViewUpdate) {
        if (
          u.docChanged ||
          u.viewportChanged ||
          u.transactions.some((tr) => tr.effects.some((e) => e.is(refreshLinks)))
        ) {
          this.decorations = buildLinks(u.view);
        }
      }
    },
    {
      decorations: (v) => v.decorations,
      // Treat a pill as one atom: the caret steps over it, Backspace deletes the whole link.
      provide: (plugin) =>
        EditorView.atomicRanges.of((v) => v.plugin(plugin)?.decorations ?? Decoration.none),
    },
  );

  // --- [[...]] autocomplete over thread titles + card labels (now desirable, not banned) ---
  function wikiCompletions(context: CompletionContext) {
    const before = context.matchBefore(/\[\[[^\[\]\n]*/);
    if (!before) return null;
    const typed = before.text.slice(2).toLowerCase();
    const all = ctx.candidates?.() ?? [];
    const options = all
      .filter((c) => c.label.toLowerCase().includes(typed))
      .slice(0, 50)
      .map((c) => ({
        label: c.label,
        detail: c.kind,
        apply: (v: EditorView, _c: unknown, from: number, to: number) => {
          const insert = `[[${c.id}]]`;
          v.dispatch({ changes: { from, to, insert }, selection: { anchor: from + insert.length } });
        },
      }));
    if (!options.length) return null;
    return { from: before.from, options, filter: false };
  }

  // --- keymaps that make card boundaries feel like one flowing document ---
  const cardKeymap = keymap.of([
    {
      key: "Mod-Enter",
      run: (v) => {
        const pos = v.state.selection.main.head;
        ctx.onSplit?.(v.state.sliceDoc(0, pos), v.state.sliceDoc(pos));
        return true;
      },
    },
    {
      key: "ArrowUp",
      run: (v) => {
        const s = v.state.selection.main;
        if (s.empty && v.state.doc.lineAt(s.head).number === 1) {
          ctx.onArrowUpOut?.();
          return true;
        }
        return false;
      },
    },
    {
      key: "ArrowDown",
      run: (v) => {
        const s = v.state.selection.main;
        if (s.empty && v.state.doc.lineAt(s.head).number === v.state.doc.lines) {
          ctx.onArrowDownOut?.();
          return true;
        }
        return false;
      },
    },
    {
      key: "Backspace",
      run: (v) => {
        const s = v.state.selection.main;
        if (s.empty && s.head === 0) {
          ctx.onMergeUp?.();
          return true;
        }
        return false;
      },
    },
  ]);

  const mdHighlight = HighlightStyle.define([
    { tag: t.heading, color: "#fff", fontWeight: "600" },
    { tag: t.strong, color: "#fff", fontWeight: "700" },
    { tag: t.emphasis, color: "#d2d6db", fontStyle: "italic" },
    { tag: t.link, color: "#8fb8ff" },
    { tag: t.url, color: "#6b7178" },
    { tag: t.monospace, color: "#c9a85f" },
    { tag: t.quote, color: "#8a9099" },
    { tag: t.list, color: "#8fb8ff" },
    { tag: [t.processingInstruction, t.meta], color: "#6b7178" },
  ]);

  // Card mode: no chrome of its own — the thread view draws the boundary + address gutter.
  const appTheme = EditorView.theme(
    {
      "&": { backgroundColor: "transparent", color: "#e6e6e6", fontSize: ".95rem" },
      "&.cm-focused": { outline: "none" },
      ".cm-scroller": { fontFamily: "ui-sans-serif, system-ui, sans-serif", lineHeight: "1.55" },
      ".cm-content": { padding: "0", caretColor: "#e6e6e6" },
      ".cm-line": { padding: "0" },
      ".cm-cursor": { borderLeftColor: "#e6e6e6" },
      ".cm-placeholder": { color: "#6b7178" },
      ".cm-linkpill": {
        color: "#8fb8ff",
        backgroundColor: "rgba(143,184,255,0.12)",
        borderRadius: "4px",
        padding: "0 4px",
        cursor: "pointer",
      },
    },
    { dark: true },
  );

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: el,
      extensions: [
        history(),
        cardKeymap,
        keymap.of([...completionKeymap, ...defaultKeymap, ...historyKeymap]),
        markdown(),
        syntaxHighlighting(mdHighlight),
        autocompletion({ override: [wikiCompletions], activateOnTyping: true, icons: false }),
        linkPills,
        EditorView.lineWrapping,
        cmPlaceholder(placeholder),
        appTheme,
        EditorView.domEventHandlers({ blur: () => (ctx.onSave?.(), false) }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) value = u.state.doc.toString();
        }),
      ],
    });
    if (autofocus) focus("end");
  });

  onDestroy(() => view?.destroy());

  /** Place the caret in this card (used when arrowing/splitting between cards). */
  export function focus(where: "start" | "end" = "end") {
    if (!view) return;
    const anchor = where === "start" ? 0 : view.state.doc.length;
    view.focus();
    view.dispatch({ selection: { anchor }, scrollIntoView: true });
  }

  // Push external changes (a thread reload, a merge) into the editor without a feedback
  // loop — when the edit originates here, value already equals the doc, so this no-ops.
  $effect(() => {
    const incoming = value;
    if (view && incoming !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: incoming } });
    }
  });
</script>

<div class="cm-host" bind:this={el}></div>

<style>
  .cm-host {
    width: 100%;
  }
</style>
