<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Template } from "$lib/api";

  let vaultPath = $state<string | null>(null);
  let templates = $state<Template[]>([]);
  let selectedId = $state<string>("");
  let title = $state("");
  let error = $state("");
  let created = $state<{ id: string; title: string } | null>(null);

  const selected = $derived(templates.find((t) => t.id === selectedId) ?? null);

  onMount(async () => {
    try {
      // the vault may already be open from the main view; reopen the saved one if not
      const saved = await api.getSavedVault();
      if (saved) {
        await api.openVault(saved);
        vaultPath = saved;
      }
      templates = await api.capture.listTemplates();
      if (templates.length) selectedId = templates[0].id;
    } catch (e) {
      error = String(e);
    }
  });

  async function create() {
    error = "";
    if (!selected || !title.trim()) return;
    try {
      const note = await api.capture.createFromTemplate(selected.id, title);
      created = { id: note.id, title: note.title };
      title = "";
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:head><title>local-roam — new from template</title></svelte:head>

<main class="page">
  <header>
    <a class="back" href="/">← notes</a>
    <h1>New from template</h1>
  </header>

  {#if !vaultPath}
    <p class="hint">No vault open. <a href="/">Open one from the notes view</a> first.</p>
  {:else}
    <p class="sub">
      A template pre-fills a body skeleton — prompts to write the idea in your own words.
      It does <strong>not</strong> create any links: connect the note afterwards with
      “Link from memory”, typed from memory and justified.
    </p>

    {#if created}
      <div class="panel done">
        <p>Created <strong>{created.title}</strong> with the template skeleton.</p>
        <p class="sub">Open it from the <a href="/">notes view</a> to fill it in.</p>
        <button class="ghost" onclick={() => (created = null)}>Create another</button>
      </div>
    {:else}
      <div class="picker">
        <label for="tpl">Template</label>
        <div class="cards">
          {#each templates as t (t.id)}
            <button
              type="button"
              class="card"
              class:active={t.id === selectedId}
              onclick={() => (selectedId = t.id)}
            >
              <span class="name">{t.name}</span>
              <span class="desc">{t.description}</span>
              {#if t.tags.length}
                <span class="tags">{#each t.tags as tag}<span class="tag">{tag}</span>{/each}</span>
              {/if}
            </button>
          {/each}
        </div>

        <label for="title">Title</label>
        <input
          id="title"
          bind:value={title}
          placeholder="Name the note…"
          onkeydown={(e) => e.key === "Enter" && create()}
        />

        <div class="bar">
          <button onclick={create} disabled={!selected || !title.trim()}>Create note</button>
        </div>
      </div>

      {#if selected}
        <section class="panel">
          <h3>Skeleton preview</h3>
          <pre class="preview">{selected.body
            .replaceAll("{{title}}", title.trim() || "<title>")
            .replaceAll("{{date}}", "<today>")}</pre>
        </section>
      {/if}
    {/if}
  {/if}

  {#if error}<p class="err">{error}</p>{/if}
</main>

<style>
  :global(body) { margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; background: #14161a; color: #e6e6e6; }
  .page { max-width: 760px; margin: 0 auto; padding: 1.2rem 1.6rem 3rem; }
  header { display: flex; align-items: baseline; gap: 1rem; }
  h1 { font-size: 1.5rem; margin: .2rem 0 .6rem; }
  .back { color: #8fb8ff; text-decoration: none; font-size: .85rem; }
  .back:hover { text-decoration: underline; }
  .sub { color: #8a9099; font-size: .9rem; max-width: 60ch; line-height: 1.5; }
  .hint { color: #6b7178; }
  .hint a, .sub a, .done a { color: #8fb8ff; }
  .picker { display: flex; flex-direction: column; gap: .4rem; margin-top: 1rem; }
  .picker label { font-size: .75rem; color: #8a9099; margin-top: .6rem; }
  .cards { display: flex; gap: .6rem; flex-wrap: wrap; }
  .card { flex: 1 1 12rem; text-align: left; display: flex; flex-direction: column; gap: .25rem; background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .7rem .8rem; cursor: pointer; color: #d2d6db; }
  .card:hover { background: #1b1f25; }
  .card.active { border-color: #3b6ea5; background: #1a2230; }
  .card .name { font-size: .95rem; font-weight: 600; color: #e6e6e6; }
  .card .desc { font-size: .78rem; color: #8a9099; }
  .tags { display: flex; gap: .3rem; flex-wrap: wrap; margin-top: .2rem; }
  .tag { font-size: .65rem; background: #232830; color: #9aa0a6; padding: 0 .35rem; border-radius: 4px; }
  input { background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .45rem .55rem; font: inherit; }
  .bar { display: flex; gap: 1rem; margin: .9rem 0; }
  .panel { background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .8rem 1rem; margin-top: 1rem; }
  .panel h3 { margin: 0 0 .5rem; font-size: .92rem; color: #c8cdd3; }
  .panel.done p { margin: .2rem 0; }
  .preview { white-space: pre-wrap; font-family: ui-monospace, monospace; font-size: .8rem; color: #b8bdc4; margin: 0; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  button:disabled { opacity: .5; cursor: default; }
  button.ghost { background: #232830; color: #c8cdd3; }
  button.ghost:hover { background: #2c333d; }
  .err { color: #e57373; font-size: .85rem; }
</style>
