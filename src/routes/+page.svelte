<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type FailedConnection, type NodeMeta, type Note, type OutLink, type RecallResult, type TagCount } from "$lib/api";
  import Editor from "$lib/Editor.svelte";
  import LinkCarousel, { CARD_MIME, flipped } from "$lib/LinkCarousel.svelte";

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
  // #20a — the tweet cap: a justification must fit in 140 chars (enforced again in
  // linker.rs::commit_edge). Counted in Unicode chars to match the backend.
  const WHY_MAX = 140;
  const whyLen = $derived(Array.from(linkWhy.trim()).length);
  // #20b — flip-to-recall carousel: recalling a title flips its card; the justify
  // step is reached by dragging a flipped card onto this note.
  let flipFlash = $state("");
  let dragCard = $state<NodeMeta | null>(null);

  // --- search (escape hatch) ---
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchResults = $state<NodeMeta[]>([]);

  // --- tags (navigation escape hatch, #18c) — like search: present, not the default ---
  let tagsOpen = $state(false);
  let allTags = $state<TagCount[]>([]);
  let activeTag = $state("");
  let tagNotes = $state<NodeMeta[]>([]);

  // --- what to review (the connections you fail most) ---
  let reviewItems = $state<FailedConnection[]>([]);

  const parseList = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);

  // a source note carries a local PDF ref — reading it is one click, no gate (#19)
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
    { href: "/graph", label: "⊹ reconstruct the graph" },
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
      await refreshReview();
    } catch (e) {
      error = String(e);
    }
  }

  async function refreshNotes() {
    notes = await api.listNotes();
  }

  async function refreshReview() {
    try {
      reviewItems = await api.whatToReview(20);
    } catch (e) {
      error = String(e);
    }
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
    // recalling logs hits/misses — refresh the "what to review" list.
    await refreshReview();
  }

  // ---- link from memory ----
  function startLink() {
    linking = true;
    linkAttempt = "";
    linkResolved = null;
    linkTried = false;
    linkWhy = "";
    linkError = "";
    flipFlash = "";
    dragCard = null;
  }
  function cancelLink() {
    linking = false;
    linkResolved = null;
    linkTried = false;
    linkAttempt = "";
    linkWhy = "";
    linkError = "";
    flipFlash = "";
    dragCard = null;
  }
  // Recalling an exact title flips its card in the carousel (#20b). Resolution stays
  // exact-or-nothing through api.resolveLink — a near miss flips nothing and reveals
  // nothing. The edge itself starts when the flipped card is dropped on this note.
  async function resolveAttempt() {
    linkError = "";
    flipFlash = "";
    const match = await api.resolveLink(linkAttempt);
    if (!match) {
      linkTried = true;
      return;
    }
    linkTried = false;
    if (match.id === note?.id) {
      linkError = "That's the note you're on — recall a different one.";
      return;
    }
    flipFlash = flipped.has(match.id)
      ? `“${match.title}” was already flipped — drag its card onto this note to link.`
      : `Flipped “${match.title}” — drag its card onto this note to link.`;
    flipped.add(match.id);
    linkAttempt = "";
  }
  async function createFromAttempt() {
    const t = linkAttempt.trim();
    if (!t) return;
    const n = await api.createNote(t, [], [], [], "");
    await refreshNotes();
    // A deliberately created note skips the drag: you generated its title yourself,
    // so its card counts as retrieved and you land straight in the justify step.
    flipped.add(n.id);
    linkResolved = { id: n.id, title: n.title, aliases: [], refs: [] };
    linkTried = true;
  }
  function allowCardDrop(e: DragEvent) {
    if (linking && !linkResolved && e.dataTransfer?.types.includes(CARD_MIME)) e.preventDefault();
  }
  function handleCardDrop(e: DragEvent) {
    const id = e.dataTransfer?.getData(CARD_MIME);
    if (!id || !linking || linkResolved) return;
    e.preventDefault();
    dragCard = null;
    const n = notes.find((x) => x.id === id);
    if (!n || !note || n.id === note.id) return;
    linkResolved = n;
    linkTried = false;
    linkError = "";
    flipFlash = "";
  }
  async function confirmLink() {
    if (!note || !linkResolved) return;
    if (!linkWhy.trim()) {
      linkError = "A link needs a one-sentence reason.";
      return;
    }
    if (whyLen > WHY_MAX) {
      linkError = `Compress it: a justification must fit in ${WHY_MAX} characters.`;
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
    <p>A notebook that makes <em>you</em> remember the connections, not the tool.</p>
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

      {#if reviewItems.length}
        <div class="review">
          <div class="review-head">what to review</div>
          <p class="sub">Connections you miss most. The reason is hidden — go re-recall it.</p>
          <ul class="reviewlist">
            {#each reviewItems as r (r.from_id + r.to_id)}
              <li>
                <button class="linkbtn" onclick={() => selectNote(r.to_id)}>
                  {r.from_title} → {r.to_title}
                </button>
                <span class="fail" title="{r.failures} misses in {r.attempts} attempts">
                  missed {r.failures}/{r.attempts}
                </span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

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

    <!-- the open note is the drop target for a flipped carousel card (#20b) -->
    <main class="editor" ondragover={allowCardDrop} ondrop={handleCardDrop}>
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
          {#if notePdf}
            <button class="ghost" onclick={openPdf} title={notePdf}>Open PDF ↗</button>
          {/if}
          <button class="ghost danger" onclick={remove}>Delete</button>
        </div>

        {#if linking}
          <div class="panel link">
            <h3>Link from memory</h3>
            {#if !linkResolved}
              <p class="sub">
                Every other note is a face-down card. Flip one by typing its exact title
                (or alias) from memory — no autocomplete — then drag the flipped card
                onto this note to link.
              </p>
              <div class="row">
                <input bind:value={linkAttempt} placeholder="recall a title to flip its card…"
                       onkeydown={(e) => e.key === "Enter" && resolveAttempt()} />
                <button onclick={resolveAttempt}>Flip</button>
                <button class="ghost" onclick={cancelLink}>Cancel</button>
              </div>
              {#if flipFlash}<p class="flash">{flipFlash}</p>{/if}
              {#if linkError}<p class="err">{linkError}</p>{/if}
              {#if linkTried && !linkResolved}
                <p class="err">No note resolved from “{linkAttempt}”.</p>
                <div class="row">
                  <button class="ghost" onclick={() => (linkTried = false)}>Try again</button>
                  <button class="ghost" onclick={createFromAttempt}>Create “{linkAttempt.trim()}” as a new note</button>
                </div>
              {/if}
              {#if dragCard}
                <div class="dropzone" role="region" aria-label="drop the card here to link"
                     ondragover={allowCardDrop} ondrop={handleCardDrop}>
                  Drop to link “{title}” → “{dragCard.title}”
                </div>
              {/if}
              <LinkCarousel {notes} excludeId={note.id} onDragChange={(c) => (dragCard = c)} />
            {:else}
              <p>Linking to <strong>{linkResolved.title}</strong>. Why are they connected?</p>
              <textarea bind:value={linkWhy} maxlength={WHY_MAX}
                        placeholder="One tweet: these connect because…"></textarea>
              <p class="counter" class:warn={whyLen > WHY_MAX - 20}>
                {WHY_MAX - whyLen} left — compression is elaboration
              </p>
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
  .counter { color: #6b7178; font-size: .72rem; margin: .2rem 0 0; text-align: right; }
  .counter.warn { color: #c9a85f; }
  .flash { color: #8fce8f; font-size: .85rem; margin: .3rem 0; }
  .dropzone { border: 1.5px dashed #3b6ea5; border-radius: 8px; padding: .8rem; margin: .5rem 0;
              text-align: center; color: #8fb8ff; font-size: .85rem; background: #16202e; }
  .review { margin-top: 1rem; border-top: 1px solid #23272e; padding-top: .7rem; }
  .review-head { font-size: .8rem; color: #c9a85f; font-weight: 600; }
  .review .sub { margin: .2rem 0 .4rem; }
  .reviewlist { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .45rem; }
  .reviewlist li { display: flex; gap: .5rem; align-items: baseline; justify-content: space-between; }
  .reviewlist .linkbtn { white-space: normal; text-align: left; }
  .fail { color: #e0a0a0; font-size: .72rem; white-space: nowrap; }
  .tiny { font-size: .72rem; margin: .2rem 0 .4rem; }
  .empty-line { color: #6b7178; font-size: .8rem; padding: .2rem 0; list-style: none; }
  .tagchips { list-style: none; display: flex; flex-wrap: wrap; gap: .3rem; padding: 0; margin: .2rem 0 .4rem; }
  .tagchip { background: #232830; color: #c8cdd3; border: none; border-radius: 12px; padding: .15rem .55rem; font-size: .78rem; cursor: pointer; display: inline-flex; align-items: baseline; gap: .3rem; }
  .tagchip:hover { background: #2c333d; }
  .tagchip.active { background: #1a2230; color: #8fb8ff; box-shadow: inset 0 0 0 1px #3b6ea5; }
  .tagchip .count { font-size: .65rem; color: #8a9099; }
</style>
