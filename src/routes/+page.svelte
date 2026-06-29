<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type NodeMeta, type Note, type OutLink, type RecallResult } from "$lib/api";
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
  let savedFlash = $state(false);

  // --- new note ---
  let creating = $state(false);
  let newTitle = $state("");

  // --- recall gate (mechanic #2) ---
  let guesses = $state<string[]>([]);
  let guessInput = $state("");
  let recall = $state<RecallResult | null>(null);

  // --- link from memory (mechanic #1) ---
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

  const parseList = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);

  onMount(async () => {
    try {
      const saved = await api.getSavedVault();
      if (saved) await openVault(saved);
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
    // reset friction flows — every open quizzes you again
    recall = null;
    guesses = [];
    guessInput = "";
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

  // ---- recall gate ----
  function addGuess() {
    const g = guessInput.trim();
    if (g && !guesses.includes(g)) guesses = [...guesses, g];
    guessInput = "";
  }
  async function submitRecall(giveUp = false) {
    if (!note) return;
    if (!giveUp && guessInput.trim()) addGuess();
    recall = await api.submitRecall(note.id, giveUp ? [] : guesses);
  }

  // ---- link from memory ----
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
    linkTried = true;
    linkResolved = await api.resolveLink(linkAttempt);
  }
  async function createFromAttempt() {
    const t = linkAttempt.trim();
    if (!t) return;
    const n = await api.createNote(t, [], [], [], "");
    await refreshNotes();
    linkResolved = { id: n.id, title: n.title, aliases: [], refs: [] };
    linkTried = true;
  }
  async function confirmLink() {
    if (!note || !linkResolved) return;
    if (!linkWhy.trim()) {
      linkError = "A link needs a one-sentence reason.";
      return;
    }
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
</script>

<svelte:head><title>local-roam</title></svelte:head>

{#if !vaultPath}
  <main class="welcome">
    <h1>local-roam</h1>
    <p>A notebook that makes <em>you</em> remember the connections, not the tool.</p>
    <button onclick={chooseVault}>Open a vault folder…</button>
    {#if error}<p class="err">{error}</p>{/if}
  </main>
{:else}
  <div class="app">
    <aside class="sidebar">
      <div class="vault-path" title={vaultPath}>{vaultPath}</div>
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
          {searchOpen ? "▾" : "▸"} search (escape hatch)
        </button>
        {#if searchOpen}
          <input placeholder="last resort…" bind:value={searchQuery} oninput={runSearch} />
          <ul>
            {#each searchResults as r (r.id)}
              <li><button onclick={() => selectNote(r.id)}>{r.title}</button></li>
            {/each}
          </ul>
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
          <button class="ghost" onclick={startLink}>Link from memory</button>
          <button class="ghost danger" onclick={remove}>Delete</button>
        </div>

        {#if linking}
          <div class="panel link">
            <h3>Link from memory</h3>
            {#if !linkResolved}
              <p class="sub">Type the exact title (or alias) of the note to link — no autocomplete.</p>
              <div class="row">
                <input bind:value={linkAttempt} placeholder="recall the title…"
                       onkeydown={(e) => e.key === "Enter" && resolveAttempt()} />
                <button onclick={resolveAttempt}>Resolve</button>
                <button class="ghost" onclick={cancelLink}>Cancel</button>
              </div>
              {#if linkTried && !linkResolved}
                <p class="err">No note resolved from “{linkAttempt}”.</p>
                <div class="row">
                  <button class="ghost" onclick={() => (linkTried = false)}>Try again</button>
                  <button class="ghost" onclick={createFromAttempt}>Create “{linkAttempt.trim()}” as a new note</button>
                </div>
              {/if}
            {:else}
              <p>Linking to <strong>{linkResolved.title}</strong>. Why are they connected?</p>
              <textarea bind:value={linkWhy} placeholder="One sentence: these connect because…"></textarea>
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
          {#if !recall}
            <p class="sub">Recall before you reveal. Which notes do you think link to this one?</p>
            <div class="row">
              <input bind:value={guessInput} placeholder="type a title and press Enter"
                     onkeydown={(e) => e.key === 'Enter' && addGuess()} />
            </div>
            {#if guesses.length}
              <ul class="chips">{#each guesses as g}<li>{g}</li>{/each}</ul>
            {/if}
            <div class="row">
              <button onclick={() => submitRecall(false)}>Reveal &amp; score</button>
              <button class="ghost" onclick={() => submitRecall(true)}>I give up</button>
            </div>
          {:else}
            <p class="score">
              <span class="hit">{recall.hits.length} recalled</span> ·
              <span class="miss">{recall.missed.length} missed</span> ·
              <span class="spur">{recall.spurious.length} wrong</span>
            </p>
            {#if recall.reveal.length}
              <ul class="links">
                {#each recall.reveal as b (b.from_id)}
                  <li class:missed={recall.missed.some((m) => m.from_id === b.from_id)}>
                    <button class="linkbtn" onclick={() => selectNote(b.from_id)}>{b.from_title}</button>
                    <span class="why">{b.why}</span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="sub">Nothing links here yet.</p>
            {/if}
            <button class="ghost small" onclick={() => { recall = null; guesses = []; }}>Try recall again</button>
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
  .links li.missed .linkbtn { color: #e0a0a0; }
  .linkbtn { background: none; border: none; color: #8fb8ff; cursor: pointer; padding: 0; font-size: .9rem; white-space: nowrap; }
  .why { color: #8a9099; font-size: .82rem; }
  .chips { list-style: none; display: flex; flex-wrap: wrap; gap: .3rem; padding: 0; margin: .3rem 0; }
  .chips li { background: #1f242b; padding: .15rem .5rem; border-radius: 12px; font-size: .8rem; }
  .score { font-size: .9rem; }
  .hit { color: #8fce8f; } .miss { color: #e0a0a0; } .spur { color: #c9a85f; }
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
</style>
