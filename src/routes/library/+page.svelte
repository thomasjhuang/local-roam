<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { api, type SourceMeta } from "$lib/api";

  let vaultPath = $state<string | null>(null);
  let sources = $state<SourceMeta[]>([]);
  let error = $state("");
  let dragging = $state(false);

  // PDFs dropped on the window, waiting for the user's name + idea. The friction
  // is the point: a PDF only becomes a source once you've said, in your own words,
  // what it is — reading it afterwards is free.
  type Pending = { path: string; file: string; name: string; idea: string; err: string };
  let pending = $state<Pending[]>([]);

  let unlisten: (() => void) | null = null;

  onMount(async () => {
    try {
      // the vault may already be open from the main view; reopen the saved one if not
      const saved = await api.getSavedVault();
      if (saved) {
        await api.openVault(saved);
        vaultPath = saved;
        await refresh();
      }
      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "over") dragging = true;
        else if (event.payload.type === "leave") dragging = false;
        else if (event.payload.type === "drop") {
          dragging = false;
          queuePdfs(event.payload.paths);
        }
      });
    } catch (e) {
      error = String(e);
    }
  });

  onDestroy(() => unlisten?.());

  async function refresh() {
    sources = await api.listSources();
  }

  function queuePdfs(paths: string[]) {
    const pdfs = paths.filter((p) => p.toLowerCase().endsWith(".pdf"));
    if (!pdfs.length) {
      error = "Only PDFs can join the library.";
      return;
    }
    error = "";
    for (const path of pdfs) {
      if (pending.some((p) => p.path === path)) continue;
      const file = path.split("/").pop() ?? path;
      pending = [...pending, { path, file, name: "", idea: "", err: "" }];
    }
  }

  async function importOne(p: Pending) {
    p.err = "";
    try {
      await api.capture.importPdf(p.path, p.name, p.idea);
      pending = pending.filter((x) => x.path !== p.path);
      await refresh();
    } catch (e) {
      p.err = String(e);
      pending = [...pending];
    }
  }

  function skip(p: Pending) {
    pending = pending.filter((x) => x.path !== p.path);
  }

  async function openPdf(id: string) {
    error = "";
    try {
      await api.openSource(id);
    } catch (e) {
      error = String(e);
    }
  }

  const day = (created: string) => created.slice(0, 10);
</script>

<svelte:head><title>local-roam — library</title></svelte:head>

<main class="page" class:dragging>
  <header>
    <a class="back" href="/">← notes</a>
    <h1>Library</h1>
  </header>

  {#if !vaultPath}
    <p class="hint">No vault open. <a href="/">Open one from the notes view</a> first.</p>
  {:else}
    <p class="sub">
      Your sources: notes with the PDF attached. <strong>Drop a PDF anywhere on this
      window</strong> to add one — it becomes a source once you've named it and written
      one sentence in your own words. Reading is free; <em>connecting</em> still happens
      from memory in the notes view.
    </p>

    {#if pending.length}
      <section class="inbox">
        {#each pending as p (p.path)}
          <div class="panel drop">
            <div class="file" title={p.path}>⤓ {p.file}</div>
            <label>
              Your name for it — not the filename
              <input bind:value={p.name} placeholder="what would you call this paper?" />
            </label>
            <label>
              The idea, one sentence
              <input
                bind:value={p.idea}
                placeholder="what is this paper to you?"
                onkeydown={(e) => e.key === "Enter" && importOne(p)}
              />
            </label>
            {#if p.err}<p class="err">{p.err}</p>{/if}
            <div class="row">
              <button onclick={() => importOne(p)} disabled={!p.name.trim() || !p.idea.trim()}>
                Add to library
              </button>
              <button class="ghost" onclick={() => skip(p)}>Skip</button>
            </div>
          </div>
        {/each}
      </section>
    {/if}

    {#if !sources.length && !pending.length}
      <div class="empty-lib">
        <p>No sources yet.</p>
        <p class="sub">Drag a PDF onto this window to start your library.</p>
      </div>
    {:else if sources.length}
      <ul class="shelf">
        {#each sources as s (s.id)}
          <li class="row-item">
            <button class="pdfbtn" title="Open the PDF in Preview" onclick={() => openPdf(s.id)}>
              PDF
            </button>
            <div class="meta-col">
              <a class="title" href={"/?note=" + s.id}>{s.title}</a>
              {#if s.idea}<span class="idea">{s.idea}</span>{/if}
            </div>
            <div class="right">
              {#if s.tags.length}
                <span class="tags">{#each s.tags as t}<span class="tag">{t}</span>{/each}</span>
              {/if}
              <span class="date">{day(s.created)}</span>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if error}<p class="err">{error}</p>{/if}
  {#if dragging}<div class="dropzone">Drop the PDF to add it to your library</div>{/if}
</main>

<style>
  :global(body) { margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; background: #14161a; color: #e6e6e6; }
  .page { max-width: 860px; margin: 0 auto; padding: 1.2rem 1.6rem 3rem; }
  header { display: flex; align-items: baseline; gap: 1rem; }
  h1 { font-size: 1.5rem; margin: .2rem 0 .6rem; }
  .back { color: #8fb8ff; text-decoration: none; font-size: .85rem; }
  .back:hover { text-decoration: underline; }
  .sub { color: #8a9099; font-size: .9rem; max-width: 64ch; line-height: 1.5; }
  .hint { color: #6b7178; }
  .hint a { color: #8fb8ff; }
  .inbox { display: flex; flex-direction: column; gap: .8rem; margin-top: 1rem; }
  .panel { background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .8rem 1rem; }
  .panel.drop { border-color: #3b6ea5; }
  .file { font-family: ui-monospace, monospace; font-size: .8rem; color: #9aa0a6; margin-bottom: .5rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  label { display: flex; flex-direction: column; gap: .25rem; font-size: .75rem; color: #8a9099; margin: .4rem 0; }
  input { background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .45rem .55rem; font: inherit; }
  .row { display: flex; gap: .5rem; margin-top: .5rem; }
  .shelf { list-style: none; margin: 1.2rem 0 0; padding: 0; display: flex; flex-direction: column; }
  .row-item { display: flex; align-items: center; gap: .8rem; padding: .55rem .4rem; border-bottom: 1px solid #1e2228; }
  .row-item:hover { background: #171b21; }
  .pdfbtn { background: #2a3b2a; color: #8fce8f; border: none; border-radius: 6px; font-size: .68rem; font-weight: 700; padding: .45rem .5rem; cursor: pointer; letter-spacing: .5px; }
  .pdfbtn:hover { background: #35502f; }
  .meta-col { display: flex; flex-direction: column; gap: .15rem; min-width: 0; flex: 1; }
  .title { color: #e6e6e6; text-decoration: none; font-size: .95rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title:hover { color: #8fb8ff; }
  .idea { color: #8a9099; font-size: .8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .right { display: flex; align-items: center; gap: .6rem; flex-shrink: 0; }
  .tags { display: flex; gap: .3rem; }
  .tag { font-size: .65rem; background: #232830; color: #9aa0a6; padding: 0 .35rem; border-radius: 4px; }
  .date { color: #6b7178; font-size: .72rem; font-variant-numeric: tabular-nums; }
  .empty-lib { margin-top: 3rem; text-align: center; color: #6b7178; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  button:disabled { opacity: .5; cursor: default; }
  button.ghost { background: #232830; color: #c8cdd3; }
  button.ghost:hover { background: #2c333d; }
  .err { color: #e57373; font-size: .85rem; }
  .dropzone { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(20, 26, 34, .85); border: 2px dashed #3b6ea5; font-size: 1.1rem; color: #8fb8ff; pointer-events: none; }
</style>
