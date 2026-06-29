<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";

  let vaultPath = $state<string | null>(null);
  let input = $state("");
  let busy = $state(false);
  let error = $state("");
  let created = $state<{ id: string; title: string } | null>(null);

  onMount(async () => {
    try {
      // the vault may already be open from the main view; reopen the saved one if not
      const saved = await api.getSavedVault();
      if (saved) {
        await api.openVault(saved);
        vaultPath = saved;
      }
    } catch (e) {
      error = String(e);
    }
  });

  async function importCitation() {
    error = "";
    if (!input.trim() || busy) return;
    busy = true;
    try {
      const note = await api.capture.importCitation(input);
      created = { id: note.id, title: note.title };
      input = "";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>local-roam — import a citation</title></svelte:head>

<main class="page">
  <header>
    <a class="back" href="/">← notes</a>
    <h1>Import a citation</h1>
  </header>

  {#if !vaultPath}
    <p class="hint">No vault open. <a href="/">Open one from the notes view</a> first.</p>
  {:else}
    <p class="sub">
      Paste a <strong>BibTeX entry</strong> or an <strong>arXiv id / URL</strong>. It becomes a
      paper note carrying the citation's refs and a body skeleton. Importing does
      <strong>not</strong> create any links: connect the paper afterwards with “Link from
      memory”, typed from memory and justified.
    </p>

    {#if created}
      <div class="panel done">
        <p>Imported <strong>{created.title}</strong> as a paper note.</p>
        <p class="sub">Open it from the <a href="/">notes view</a> to write your summary.</p>
        <button class="ghost" onclick={() => (created = null)}>Import another</button>
      </div>
    {:else}
      <div class="form">
        <label for="src">BibTeX entry or arXiv id/URL</label>
        <textarea
          id="src"
          bind:value={input}
          placeholder={"@article{...}\n\nor\n\narXiv:1706.03762  ·  https://arxiv.org/abs/1706.03762"}
        ></textarea>
        <div class="bar">
          <button onclick={importCitation} disabled={!input.trim() || busy}>
            {busy ? "Importing…" : "Import"}
          </button>
        </div>
      </div>
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
  .form { display: flex; flex-direction: column; gap: .4rem; margin-top: 1rem; }
  .form label { font-size: .75rem; color: #8a9099; }
  textarea { width: 100%; box-sizing: border-box; min-height: 10rem; resize: vertical; background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .6rem .7rem; font: inherit; font-family: ui-monospace, monospace; font-size: .82rem; line-height: 1.5; }
  .bar { display: flex; gap: 1rem; margin: .9rem 0; }
  .panel { background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .8rem 1rem; margin-top: 1rem; }
  .panel.done p { margin: .2rem 0; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  button:disabled { opacity: .5; cursor: default; }
  button.ghost { background: #232830; color: #c8cdd3; }
  button.ghost:hover { background: #2c333d; }
  .err { color: #e57373; font-size: .85rem; }
</style>
