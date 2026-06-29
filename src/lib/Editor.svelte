<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    EditorView,
    keymap,
    placeholder as cmPlaceholder,
    Decoration,
    ViewPlugin,
    MatchDecorator,
    type DecorationSet,
    type ViewUpdate,
  } from "@codemirror/view";
  import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
  import { markdown } from "@codemirror/lang-markdown";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { tags as t } from "@lezer/highlight";

  interface Props {
    value: string;
    placeholder?: string;
  }
  let { value = $bindable(), placeholder = "" }: Props = $props();

  let el: HTMLDivElement;
  let view: EditorView | undefined;

  // --- [[wiki-link]] rendering (display-only) ---
  // Per CONTEXT.md, this is purely visual: it highlights existing [[...]] spans.
  // It deliberately does NOT suggest, resolve, or autocomplete targets — linking
  // still goes exclusively through the no-autocomplete "Link from memory" flow.
  const wikiLinkDeco = Decoration.mark({ class: "cm-wikilink" });
  const wikiLinkMatcher = new MatchDecorator({
    regexp: /\[\[([^\[\]\n]+)\]\]/g,
    decoration: wikiLinkDeco,
  });
  const wikiLinks = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(v: EditorView) {
        this.decorations = wikiLinkMatcher.createDeco(v);
      }
      update(u: ViewUpdate) {
        this.decorations = wikiLinkMatcher.updateDeco(u, this.decorations);
      }
    },
    { decorations: (v) => v.decorations },
  );

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

  const appTheme = EditorView.theme(
    {
      "&": {
        minHeight: "30vh",
        backgroundColor: "#1a1d22",
        color: "#e6e6e6",
        border: "1px solid #262a30",
        borderRadius: "8px",
        fontSize: ".95rem",
      },
      "&.cm-focused": { outline: "none", borderColor: "#3b6ea5" },
      ".cm-scroller": {
        fontFamily: "ui-sans-serif, system-ui, sans-serif",
        lineHeight: "1.5",
      },
      ".cm-content": { padding: ".8rem", caretColor: "#e6e6e6" },
      ".cm-cursor": { borderLeftColor: "#e6e6e6" },
      ".cm-placeholder": { color: "#6b7178" },
      ".cm-wikilink": {
        color: "#8fb8ff",
        backgroundColor: "rgba(143,184,255,0.10)",
        borderRadius: "3px",
        padding: "0 1px",
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
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown(),
        syntaxHighlighting(mdHighlight),
        wikiLinks,
        EditorView.lineWrapping,
        cmPlaceholder(placeholder),
        appTheme,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) value = u.state.doc.toString();
        }),
      ],
    });
  });

  onDestroy(() => view?.destroy());

  // Push external changes (e.g. selecting a different note) into the editor.
  // When the edit originates here, value already equals the doc, so this no-ops —
  // no feedback loop.
  $effect(() => {
    const incoming = value;
    if (view && incoming !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: incoming },
      });
    }
  });
</script>

<div class="cm-host" bind:this={el}></div>

<style>
  .cm-host {
    width: 100%;
  }
</style>
