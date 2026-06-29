<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Note } from "$lib/api";

  let vaultPath = $state<string | null>(null);
  let note = $state<Note | null>(null);
  let body = $state("");
  let error = $state("");
  let savedFlash = $state(false);

  onMount(async () => {
    try {
      // the vault may already be open from the main view; reopen the saved one if not
      const saved = await api.getSavedVault();
      if (saved) {
        await api.openVault(saved);
        vaultPath = saved;
        note = await api.capture.openDailyNote();
        body = note.body;
      }
    } catch (e) {
      error = String(e);
    }
  });

  async function save() {
    if (!note) return;
    error = "";
    try {
      const updated = await api.saveNote(
        note.id, note.title, body, note.refs, note.aliases, note.tags,
      );
      note = { ...updated, body };
      savedFlash = true;
      setTimeout(() => (savedFlash = false), 1200);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:head><title>local-roam — today's note</title></svelte:head>

<main class="page">
  <header>
    <a class="back" href="/">← notes</a>
    <h1>Today's note{#if note} · {note.title}{/if}</h1>
  </header>

  {#if !vaultPath}
    <p class="hint">No vault open. <a href="/">Open one from the notes view</a> first.</p>
  {:else if note}
    <p class="sub">
      A dated scratchpad — capture fast now, process later. This still doesn't make any
      links: turn anything worth keeping into its own note and connect it with
      “Link from memory”.
    </p>

    <textarea bind:value={body} placeholder="Jot a fleeting thought…"></textarea>
    <div class="bar">
      <button onclick={save}>{savedFlash ? "Saved ✓" : "Save"}</button>
    </div>
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
  .hint a { color: #8fb8ff; }
  textarea { width: 100%; box-sizing: border-box; min-height: 18rem; resize: vertical; margin-top: 1rem; background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .6rem .7rem; font: inherit; line-height: 1.5; }
  .bar { display: flex; gap: 1rem; margin: .9rem 0; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  .err { color: #e57373; font-size: .85rem; }
</style>
