<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";

  let vaultPath = $state<string | null>(null);
  let url = $state("");
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

  async function clip() {
    error = "";
    if (!url.trim() || busy) return;
    busy = true;
    try {
      const note = await api.capture.clipUrl(url);
      created = { id: note.id, title: note.title };
      url = "";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>local-roam — clip a URL</title></svelte:head>

<main class="page">
  <header>
    <a class="back" href="/">← notes</a>
    <h1>Clip a URL</h1>
  </header>

  {#if !vaultPath}
    <p class="hint">No vault open. <a href="/">Open one from the notes view</a> first.</p>
  {:else}
    <p class="sub">
      Paste a URL. local-roam fetches the page and saves a note with its title, the URL as a
      ref, and the extracted readable text. Clipping does <strong>not</strong> create any
      links: turn what matters into your own words and connect it with “Link from memory”.
    </p>

    {#if created}
      <div class="panel done">
        <p>Clipped <strong>{created.title}</strong> into a note.</p>
        <p class="sub">Open it from the <a href="/">notes view</a> to work it over.</p>
        <button class="ghost" onclick={() => (created = null)}>Clip another</button>
      </div>
    {:else}
      <div class="form">
        <label for="url">Page URL</label>
        <input
          id="url"
          bind:value={url}
          placeholder="https://example.com/article"
          onkeydown={(e) => e.key === "Enter" && clip()}
        />
        <div class="bar">
          <button onclick={clip} disabled={!url.trim() || busy}>
            {busy ? "Clipping…" : "Clip"}
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
  input { width: 100%; box-sizing: border-box; background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .45rem .55rem; font: inherit; }
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
