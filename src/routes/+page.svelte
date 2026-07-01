<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type Backlink, type NodeMeta, type Note, type OutLink, type TagCount } from "$lib/api";
  import Editor from "$lib/Editor.svelte";

  // --- vault ---
  let vaultPath = $state<string | null>(null);
  let notes = $state<NodeMeta[]>([]);
  let error = $state<string>("");

  // --- selection / editor ---
  let note = $state<Note | null>(null);
  let title = $state("");
  let body = $state("");
  let refsStr = $state("");
  let aliasesStr = $state("");
  let tagsStr = $state("");
  let outgoing = $state<OutLink[]>([]);
  let backlinks = $state<Backlink[]>([]);
  let savedFlash = $state(false);

  // --- new note ---
  let creating = $state(false);
  let newTitle = $state("");

  // --- link flow (stopgap until the card/thread model) ---
  let linking = $state(false);
  let linkAttempt = $state("");
  let linkResolved = $state<NodeMeta | null>(null);
  let linkTried = $state(false);
  let linkWhy = $state("");
  let linkError = $state("");

  // --- search (escape hatch) ---
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchResults = $state<NodeMeta[]>([]);

  // --- tags (navigation escape hatch, #18c) — like search: present, not the default ---
  let tagsOpen = $state(false);
  let allTags = $state<TagCount[]>([]);
  let activeTag = $state("");
  let tagNotes = $state<NodeMeta[]>([]);

  const parseList = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);

  // a source note carries a local PDF ref — reading it is one click (#19)
  const notePdf = $derived(
    note?.refs.find((r) => r.trim().toLowerCase().endsWith(".pdf") && !r.includes("://")) ?? null,
  );
  async function openPdf() {
    if (!note) return;
    try {
      await api.openSource(note.id);
    } catch (e) {
      error = String(e);
    }
  }

  // --- sidebar nav (data-driven; capture slices append entries, append-only) ---
  const navLinks: { href: string; label: string }[] = [
    { href: "/capture/template", label: "✚ new from template" },
    { href: "/capture/daily", label: "✎ today's note" },
    { href: "/capture/import", label: "⇲ import a citation" },
    { href: "/capture/clip", label: "✂ clip a URL" },
    { href: "/library", label: "▤ library — your PDFs" },
  ];

  onMount(async () => {
    try {
      const saved = await api.getSavedVault();
      if (saved) await openVault(saved);
      // deep link from the library (/?note=<id>): open that note directly.
      const wanted = new URLSearchParams(location.search).get("note");
      if (wanted && notes.some((n) => n.id === wanted)) await selectNote(wanted);
    } catch (e) {
      error = String(e);
    }
  });

  async function chooseVault() {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") await openVault(picked);
  }

  async function openVault(path: string) {
    try {
      await api.openVault(path);
      vaultPath = path;
      await refreshNotes();
    } catch (e) {
      error = String(e);
    }
  }

  async function refreshNotes() {
    notes = await api.listNotes();
  }

  async function selectNote(id: string) {
    error = "";
    note = await api.getNote(id);
    title = note.title;
    body = note.body;
    refsStr = note.refs.join(", ");
    aliasesStr = note.aliases.join(", ");
    tagsStr = note.tags.join(", ");
    outgoing = await api.outgoing(id);
    backlinks = await api.backlinks(id);
    cancelLink();
  }

  async function createNote() {
    const t = newTitle.trim();
    if (!t) return;
    const n = await api.createNote(t, [], [], [], "");
    creating = false;
    newTitle = "";
    await refreshNotes();
    await selectNote(n.id);
  }

  async function save() {
    if (!note) return;
    const updated = await api.saveNote(
      note.id, title, body, parseList(refsStr), parseList(aliasesStr), parseList(tagsStr),
    );
    note = { ...updated, body };
    await refreshNotes();
    savedFlash = true;
    setTimeout(() => (savedFlash = false), 1200);
  }

  async function remove() {
    if (!note) return;
    if (!confirm(`Delete "${note.title}"? This cannot be undone.`)) return;
    await api.deleteNote(note.id);
    note = null;
    await refreshNotes();
  }

  // ---- link flow ----
  function startLink() {
    linking = true;
    linkAttempt = "";
    linkResolved = null;
    linkTried = false;
    linkWhy = "";
    linkError = "";
  }
  function cancelLink() {
    linking = false;
    linkResolved = null;
    linkTried = false;
    linkAttempt = "";
    linkWhy = "";
    linkError = "";
  }
  async function resolveAttempt() {
    linkError = "";
    const match = await api.resolveLink(linkAttempt);
    if (!match) {
      linkTried = true;
      return;
    }
    if (match.id === note?.id) {
      linkError = "That's the note you're on — pick a different one.";
      return;
    }
    linkResolved = match;
    linkTried = false;
  }
  async function createFromAttempt() {
    const t = linkAttempt.trim();
    if (!t) return;
    const n = await api.createNote(t, [], [], [], "");
    await refreshNotes();
    linkResolved = { id: n.id, title: n.title, aliases: [], refs: [] };
    linkTried = false;
  }
  async function confirmLink() {
    if (!note || !linkResolved) return;
    try {
      await api.commitLink(note.id, linkResolved.id, linkWhy);
      outgoing = await api.outgoing(note.id);
      cancelLink();
    } catch (e) {
      linkError = String(e);
    }
  }

  // ---- search ----
  async function runSearch() {
    searchResults = searchQuery.trim() ? await api.search(searchQuery) : [];
  }

  // ---- tags (navigation escape hatch) ----
  async function toggleTags() {
    tagsOpen = !tagsOpen;
    activeTag = "";
    tagNotes = [];
    if (tagsOpen) allTags = await api.listTags(); // re-fetch on open so counts stay fresh
  }
  async function browseTag(tag: string) {
    activeTag = tag;
    tagNotes = await api.notesByTag(tag);
  }
</script>

<svelte:head><title>local-roam</title></svelte:head>

{#if !vaultPath}
  <main class="welcome">
    <h1>local-roam</h1>
    <p>Atomic cards, assembled into readable threads, with the paper right there.</p>
    <button onclick={chooseVault}>Open a vault folder…</button>
    {#if error}<p class="err">{error}</p>{/if}
  </main>
{:else}
  <div class="app">
    <aside class="sidebar">
      <div class="vault-path" title={vaultPath}>{vaultPath}</div>
      {#each navLinks as link (link.href)}
        <a class="navlink" href={link.href}>{link.label}</a>
      {/each}
      {#if creating}
        <div class="newnote">
          <input placeholder="New note title…" bind:value={newTitle}
                 onkeydown={(e) => e.key === "Enter" && createNote()} />
          <div class="row">
            <button onclick={createNote}>Create</button>
            <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
          </div>
        </div>
      {:else}
        <button class="full" onclick={() => { creating = true; newTitle = ""; }}>+ New note</button>
      {/if}

      <ul class="notelist">
        {#each notes as n (n.id)}
          <li class:active={note?.id === n.id}>
            <button onclick={() => selectNote(n.id)}>
              {n.title}
              {#if n.refs.length}<span class="ref">ref</span>{/if}
            </button>
          </li>
        {/each}
        {#if !notes.length}<li class="empty">No notes yet.</li>{/if}
      </ul>

      <div class="search">
        <button class="ghost small" onclick={() => (searchOpen = !searchOpen)}>
          {searchOpen ? "▾" : "▸"} search
        </button>
        {#if searchOpen}
          <input placeholder="search titles + bodies…" bind:value={searchQuery} oninput={runSearch} />
          <ul>
            {#each searchResults as r (r.id)}
              <li><button onclick={() => selectNote(r.id)}>{r.title}</button></li>
            {/each}
          </ul>
        {/if}
      </div>

      <div class="tagsnav">
        <button class="ghost small" onclick={toggleTags}>
          {tagsOpen ? "▾" : "▸"} browse by tag
        </button>
        {#if tagsOpen}
          <p class="sub tiny">An escape hatch, like search — not how you'd normally find a note.</p>
          {#if !allTags.length}
            <p class="empty-line">No tags yet.</p>
          {:else}
            <ul class="tagchips">
              {#each allTags as t (t.tag)}
                <li>
                  <button class="tagchip" class:active={t.tag === activeTag} onclick={() => browseTag(t.tag)}>
                    {t.tag}<span class="count">{t.count}</span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if activeTag}
              <ul>
                {#each tagNotes as n (n.id)}
                  <li><button onclick={() => selectNote(n.id)}>{n.title}</button></li>
                {/each}
                {#if !tagNotes.length}<li class="empty-line">No notes tagged “{activeTag}”.</li>{/if}
              </ul>
            {/if}
          {/if}
        {/if}
      </div>
    </aside>

    <main class="editor">
      {#if !note}
        <p class="hint">Select or create a note.</p>
      {:else}
        <input class="title" bind:value={title} />
        <div class="meta">
          <label>refs <input bind:value={refsStr} placeholder="arXiv:…, doi:…" /></label>
          <label>aliases <input bind:value={aliasesStr} placeholder="comma, separated" /></label>
          <label>tags <input bind:value={tagsStr} placeholder="comma, separated" /></label>
        </div>
        <Editor bind:value={body} placeholder="Write the idea in your own words…" />
        <div class="actions">
          <button onclick={save}>{savedFlash ? "Saved ✓" : "Save"}</button>
          <button class="ghost" onclick={startLink}>Link…</button>
          {#if notePdf}
            <button class="ghost" onclick={openPdf} title={notePdf}>Open PDF ↗</button>
          {/if}
          <button class="ghost danger" onclick={remove}>Delete</button>
        </div>

        {#if linking}
          <div class="panel link">
            <h3>Link to another note</h3>
            {#if !linkResolved}
              <p class="sub">Type the title (or alias) of the note to link to.</p>
              <div class="row">
                <input bind:value={linkAttempt} placeholder="a title…"
                       onkeydown={(e) => e.key === "Enter" && resolveAttempt()} />
                <button onclick={resolveAttempt}>Find</button>
                <button class="ghost" onclick={cancelLink}>Cancel</button>
              </div>
              {#if linkError}<p class="err">{linkError}</p>{/if}
              {#if linkTried}
                <p class="err">No note titled “{linkAttempt}”.</p>
                <div class="row">
                  <button class="ghost" onclick={() => (linkTried = false)}>Try again</button>
                  <button class="ghost" onclick={createFromAttempt}>Create “{linkAttempt.trim()}” as a new note</button>
                </div>
              {/if}
            {:else}
              <p>Linking to <strong>{linkResolved.title}</strong>. Add an optional reason:</p>
              <textarea bind:value={linkWhy} placeholder="why are they connected? (optional)"></textarea>
              {#if linkError}<p class="err">{linkError}</p>{/if}
              <div class="row">
                <button onclick={confirmLink}>Commit link</button>
                <button class="ghost" onclick={cancelLink}>Cancel</button>
              </div>
            {/if}
          </div>
        {/if}

        {#if outgoing.length}
          <div class="panel">
            <h3>Links you made →</h3>
            <ul class="links">
              {#each outgoing as o (o.to_id)}
                <li><button class="linkbtn" onclick={() => selectNote(o.to_id)}>{o.to_title}</button><span class="why">{o.why}</span></li>
              {/each}
            </ul>
          </div>
        {/if}

        <div class="panel backlinks">
          <h3>← What links here?</h3>
          {#if backlinks.length}
            <ul class="links">
              {#each backlinks as b (b.from_id)}
                <li>
                  <button class="linkbtn" onclick={() => selectNote(b.from_id)}>{b.from_title}</button>
                  <span class="why">{b.why}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="sub">Nothing links here yet.</p>
          {/if}
        </div>
      {/if}
      {#if error}<p class="err">{error}</p>{/if}
    </main>
  </div>
{/if}

<style>
  :global(body) { margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; background: #14161a; color: #e6e6e6; }
  .welcome { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100vh; gap: 1rem; text-align: center; }
  .welcome h1 { font-size: 3rem; margin: 0; letter-spacing: -1px; }
  .welcome p { color: #9aa0a6; max-width: 26rem; }
  .app { display: grid; grid-template-columns: 280px 1fr; height: 100vh; }
  .sidebar { border-right: 1px solid #262a30; display: flex; flex-direction: column; padding: .6rem; gap: .6rem; overflow: hidden; }
  .vault-path { font-size: .7rem; color: #6b7178; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .navlink { font-size: .8rem; color: #8fb8ff; text-decoration: none; padding: .1rem 0; }
  .navlink:hover { text-decoration: underline; }
  .notelist { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; }
  .notelist li button { width: 100%; text-align: left; background: none; border: none; color: #d2d6db; padding: .4rem .5rem; border-radius: 6px; cursor: pointer; font-size: .9rem; }
  .notelist li.active button, .notelist li button:hover { background: #1f242b; }
  .notelist li.empty { color: #6b7178; font-size: .85rem; padding: .4rem; }
  .ref { font-size: .6rem; background: #2a3b2a; color: #8fce8f; padding: 0 .3rem; border-radius: 4px; margin-left: .4rem; }
  .newnote input, .search input { width: 100%; box-sizing: border-box; }
  .editor { padding: 1.2rem 1.6rem; overflow-y: auto; }
  .title { font-size: 1.6rem; font-weight: 600; width: 100%; background: none; border: none; color: #fff; border-bottom: 1px solid #262a30; padding: .2rem 0; box-sizing: border-box; }
  .meta { display: flex; gap: 1rem; margin: .6rem 0; flex-wrap: wrap; }
  .meta label { font-size: .7rem; color: #6b7178; display: flex; flex-direction: column; gap: .2rem; }
  .actions { display: flex; gap: .5rem; margin: .8rem 0; }
  .panel { background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .9rem 1rem; margin-top: 1rem; }
  .panel h3 { margin: 0 0 .5rem; font-size: .95rem; color: #c8cdd3; }
  .panel.backlinks { border-color: #2d3340; }
  .sub { color: #8a9099; font-size: .85rem; margin: .3rem 0; }
  .row { display: flex; gap: .5rem; align-items: center; margin: .4rem 0; flex-wrap: wrap; }
  .links { list-style: none; margin: .4rem 0 0; padding: 0; display: flex; flex-direction: column; gap: .4rem; }
  .links li { display: flex; gap: .6rem; align-items: baseline; }
  .linkbtn { background: none; border: none; color: #8fb8ff; cursor: pointer; padding: 0; font-size: .9rem; white-space: nowrap; }
  .why { color: #8a9099; font-size: .82rem; }
  input, textarea { background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .4rem .5rem; font: inherit; }
  textarea { width: 100%; box-sizing: border-box; min-height: 4rem; resize: vertical; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  button.ghost { background: #232830; color: #c8cdd3; }
  button.ghost:hover { background: #2c333d; }
  button.ghost.small { font-size: .75rem; padding: .3rem .5rem; }
  button.full { width: 100%; }
  button.danger { color: #e0a0a0; }
  .hint { color: #6b7178; }
  .err { color: #e57373; font-size: .85rem; }
  .tiny { font-size: .72rem; margin: .2rem 0 .4rem; }
  .empty-line { color: #6b7178; font-size: .8rem; padding: .2rem 0; list-style: none; }
  .tagchips { list-style: none; display: flex; flex-wrap: wrap; gap: .3rem; padding: 0; margin: .2rem 0 .4rem; }
  .tagchip { background: #232830; color: #c8cdd3; border: none; border-radius: 12px; padding: .15rem .55rem; font-size: .78rem; cursor: pointer; display: inline-flex; align-items: baseline; gap: .3rem; }
  .tagchip:hover { background: #2c333d; }
  .tagchip.active { background: #1a2230; color: #8fb8ff; box-shadow: inset 0 0 0 1px #3b6ea5; }
  .tagchip .count { font-size: .65rem; color: #8a9099; }
</style>
