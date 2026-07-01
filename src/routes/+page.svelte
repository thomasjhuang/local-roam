<script lang="ts">
  import { onMount, tick } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    type CardMeta,
    type CardRef,
    type Placement,
    type ThreadFull,
    type ThreadMeta,
  } from "$lib/api";
  import Editor, { type LinkCandidate } from "$lib/Editor.svelte";

  type EditorInstance = { focus: (where?: "start" | "end") => void };

  // --- vault ---
  let vaultPath = $state<string | null>(null);
  let error = $state<string>("");

  // --- the two-level model: threads (titled) made of cards (untitled atoms) ---
  let threads = $state<ThreadMeta[]>([]);
  let allCards = $state<CardMeta[]>([]);
  // id → display label, so a [[id]] renders as the thread title / card first line.
  let threadsById = $state<Record<string, string>>({});
  let cardsById = $state<Record<string, string>>({});
  // bumped whenever the label maps change, to re-render the editors' link pills.
  let linkVersion = $state(0);

  // --- the open thread ---
  let thread = $state<ThreadFull | null>(null);
  let backlinks = $state<CardRef[]>([]);
  let editors = $state<Record<string, EditorInstance | undefined>>({});
  // the placement gesture in flight: a ⌘⏎ split awaiting continue / branch / new-thread.
  let pendingSplit = $state<{ cardId: string; head: string; tail: string } | null>(null);

  // --- new thread (retired by the omnibar in #25) ---
  let creating = $state(false);
  let newTitle = $state("");

  // --- sidebar nav (data-driven; capture slices append entries, append-only) ---
  const navLinks: { href: string; label: string }[] = [
    { href: "/capture/template", label: "✚ new from template" },
    { href: "/capture/daily", label: "✎ today's note" },
    { href: "/capture/import", label: "⇲ import a citation" },
    { href: "/capture/clip", label: "✂ clip a URL" },
    { href: "/library", label: "▤ library — your PDFs" },
  ];

  // --- link resolution / autocomplete (fuzzy is desirable now, recall is retired) ---
  function resolveLabel(id: string): string | null {
    return threadsById[id] ?? cardsById[id] ?? null;
  }
  function candidates(): LinkCandidate[] {
    return [
      ...threads.map((t) => ({ id: t.id, label: t.title, kind: "thread" as const })),
      ...allCards.map((c) => ({ id: c.id, label: c.label || "(untitled card)", kind: "card" as const })),
    ];
  }

  /** A card's nesting depth, read from its derived address (`2a1a` → 3). Drives indent. */
  function depthOf(address: string): number {
    const groups = address.match(/\d+|[a-z]+/g) ?? [];
    return Math.max(0, groups.length - 1);
  }

  onMount(async () => {
    try {
      const saved = await api.getSavedVault();
      if (saved) await openVault(saved);
      // deep link: /?thread=<id> (or a migrated note's id via ?note=) opens that thread.
      const params = new URLSearchParams(location.search);
      const wanted = params.get("thread") ?? params.get("note");
      if (wanted) await openTarget(wanted);
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
      await refreshMaps();
    } catch (e) {
      error = String(e);
    }
  }

  /** Reload the thread list + label maps (labels, autocomplete) after any change. */
  async function refreshMaps() {
    const [th, cd] = await Promise.all([api.listThreads(), api.listCards()]);
    threads = th;
    allCards = cd;
    threadsById = Object.fromEntries(th.map((t) => [t.id, t.title]));
    cardsById = Object.fromEntries(cd.map((c) => [c.id, c.label]));
    linkVersion++;
  }

  /** Open a thread for reading + writing, optionally placing the caret in one card. */
  async function openThread(id: string, focusCard: string | null = null) {
    error = "";
    thread = await api.getThread(id);
    backlinks = await api.cardBacklinks(id);
    pendingSplit = null;
    await tick();
    if (focusCard) editors[focusCard]?.focus("end");
  }

  /** Follow a link/backlink: a thread id opens directly; a card id opens a thread it sits in. */
  async function openTarget(id: string) {
    if (threadsById[id]) return openThread(id, null);
    try {
      const mem = await api.cardMemberships(id);
      if (mem.length) await openThread(mem[0].thread_id, id);
      else error = "That card isn't in any thread yet.";
    } catch (e) {
      error = String(e);
    }
  }

  async function createThread() {
    const t = newTitle.trim();
    if (!t) return;
    try {
      const id = await api.newThread(t);
      creating = false;
      newTitle = "";
      await refreshMaps();
      await openThread(id, null);
    } catch (e) {
      error = String(e);
    }
  }

  async function renameThread() {
    if (!thread) return;
    try {
      await api.renameThread(thread.id, thread.title);
      await refreshMaps();
    } catch (e) {
      error = String(e);
    }
  }

  // ---- writing through cards ----
  async function saveCard(cardId: string) {
    const card = thread?.cards.find((c) => c.card_id === cardId);
    if (!card) return;
    try {
      await api.saveCard(cardId, card.body);
      await refreshMaps(); // a card's first line (its label) may have changed
    } catch (e) {
      error = String(e);
    }
  }

  // ---- the placement gesture: a new card is born continuing, branching, or starting anew ----
  function beginSplit(cardId: string, head: string, tail: string) {
    pendingSplit = { cardId, head, tail };
  }
  async function placeSplit(placement: Placement) {
    if (!thread || !pendingSplit) return;
    const { cardId, head, tail } = pendingSplit;
    let title: string | null = null;
    if (placement === "new_thread") {
      title = window.prompt("Title for the new thread:")?.trim() ?? "";
      if (!title) {
        pendingSplit = null;
        return;
      }
    }
    try {
      const res = await api.splitCard(thread.id, cardId, head, tail, placement, title);
      pendingSplit = null;
      await refreshMaps();
      // new-thread lifts the tail elsewhere: keep the caret on the (truncated) source here.
      await openThread(thread.id, placement === "new_thread" ? cardId : res.card_id);
    } catch (e) {
      error = String(e);
    }
  }
  function cancelSplit() {
    pendingSplit = null;
  }

  /** Add a blank card by the gesture: `continue` (trunk end) or `branch` off a card. */
  async function addCard(placement: Placement, anchorCardId: string | null) {
    if (!thread) return;
    try {
      const id = await api.addCard(thread.id, anchorCardId, placement, "");
      await refreshMaps();
      await openThread(thread.id, id);
    } catch (e) {
      error = String(e);
    }
  }

  async function mergeUp(cardId: string) {
    if (!thread) return;
    try {
      const prev = await api.mergeCardUp(thread.id, cardId);
      if (!prev) return; // first card — nothing above to merge into
      await refreshMaps();
      await openThread(thread.id, prev);
    } catch (e) {
      error = String(e);
    }
  }

  /** Arrow past a card boundary → move the caret into the adjacent card. */
  function focusSibling(cardId: string, dir: 1 | -1) {
    const i = thread?.cards.findIndex((c) => c.card_id === cardId) ?? -1;
    if (i < 0 || !thread) return;
    const next = thread.cards[i + dir];
    if (next) editors[next.card_id]?.focus(dir > 0 ? "start" : "end");
  }

  const paperRefs = $derived(thread?.refs.filter((r) => r.trim()) ?? []);
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
          <input
            placeholder="New thread title…"
            bind:value={newTitle}
            onkeydown={(e) => e.key === "Enter" && createThread()}
          />
          <div class="row">
            <button onclick={createThread}>Create</button>
            <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
          </div>
        </div>
      {:else}
        <button class="full" onclick={() => { creating = true; newTitle = ""; }}>+ New thread</button>
      {/if}

      <ul class="notelist">
        {#each threads as t (t.id)}
          <li class:active={thread?.id === t.id}>
            <button onclick={() => openThread(t.id, null)}>
              <span class="tlabel">{t.title}</span>
              {#if t.refs.length}<span class="ref">paper</span>{/if}
              <span class="count">{t.card_count}</span>
            </button>
          </li>
        {/each}
        {#if !threads.length}<li class="empty">No threads yet.</li>{/if}
      </ul>
    </aside>

    <main class="reader">
      {#if !thread}
        <p class="hint">Select or create a thread.</p>
      {:else}
        <input class="title" bind:value={thread.title} onblur={renameThread}
               onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        {#if paperRefs.length}
          <div class="refs">
            {#each paperRefs as r (r)}<span class="refchip">{r}</span>{/each}
          </div>
        {/if}

        <div class="thread">
          {#each thread.cards as card, i (card.card_id)}
            <div class="card" style="--depth: {depthOf(card.address)}">
              <div class="addr" title="Folgezettel address">{card.address}</div>
              <div class="cardbody">
                <Editor
                  bind:value={card.body}
                  bind:this={editors[card.card_id]}
                  placeholder={i === 0 ? "Write the first atom of this thread…" : "…"}
                  {resolveLabel}
                  {candidates}
                  {linkVersion}
                  onSave={() => saveCard(card.card_id)}
                  onSplit={(head, tail) => beginSplit(card.card_id, head, tail)}
                  onArrowUpOut={() => focusSibling(card.card_id, -1)}
                  onArrowDownOut={() => focusSibling(card.card_id, 1)}
                  onMergeUp={() => mergeUp(card.card_id)}
                  onNavigate={openTarget}
                />
                <button
                  class="branch"
                  title="Branch a new card off this one"
                  onclick={() => addCard("branch", card.card_id)}>↳ branch</button
                >
              </div>

              {#if pendingSplit?.cardId === card.card_id}
                <div class="placement">
                  <span class="ptitle">Where does the new card go?</span>
                  <button onclick={() => placeSplit("continue")}>Continue the thread</button>
                  <button onclick={() => placeSplit("branch")}>Branch off this card</button>
                  <button onclick={() => placeSplit("new_thread")}>Start a new thread</button>
                  <button class="ghost" onclick={cancelSplit}>Cancel</button>
                </div>
              {/if}
            </div>
          {/each}

          <button class="addcard" onclick={() => addCard("continue", null)}>+ add a card</button>
          <p class="tip">⌘⏎ inside a card splits it at the caret into a new card.</p>
        </div>

        <div class="panel backlinks">
          <h3>← Cards that link here</h3>
          {#if backlinks.length}
            <ul class="links">
              {#each backlinks as b (b.card_id)}
                <li>
                  <button
                    class="linkbtn"
                    onclick={() => (b.thread_id ? openThread(b.thread_id, b.card_id) : openTarget(b.card_id))}
                  >
                    {b.label || "(untitled card)"}
                  </button>
                  {#if b.thread_title}
                    <span class="why">in {b.thread_title}{b.address ? ` · ${b.address}` : ""}</span>
                  {/if}
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
  .notelist li button { width: 100%; text-align: left; background: none; border: none; color: #d2d6db; padding: .4rem .5rem; border-radius: 6px; cursor: pointer; font-size: .9rem; display: flex; align-items: center; gap: .4rem; }
  .notelist li.active button, .notelist li button:hover { background: #1f242b; }
  .notelist li.empty { color: #6b7178; font-size: .85rem; padding: .4rem; }
  .tlabel { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ref { font-size: .6rem; background: #2a3b2a; color: #8fce8f; padding: 0 .3rem; border-radius: 4px; }
  .count { font-size: .65rem; color: #6b7178; }
  .newnote input { width: 100%; box-sizing: border-box; }

  .reader { padding: 1.2rem 1.6rem; overflow-y: auto; }
  .title { font-size: 1.6rem; font-weight: 600; width: 100%; background: none; border: none; color: #fff; border-bottom: 1px solid #262a30; padding: .2rem 0; box-sizing: border-box; }
  .refs { display: flex; flex-wrap: wrap; gap: .4rem; margin: .5rem 0; }
  .refchip { font-size: .7rem; background: #1a2230; color: #8fb8ff; padding: .1rem .5rem; border-radius: 10px; }

  /* the thread reads top-to-bottom as flowing prose; card boundaries stay visible */
  .thread { margin: 1rem 0; }
  .card { position: relative; display: grid; grid-template-columns: 2.6rem 1fr; gap: .4rem;
          padding: .5rem 0; border-top: 1px solid #20242b; margin-left: calc(var(--depth) * 1.4rem); }
  .card:first-child { border-top: none; }
  .addr { font-size: .68rem; color: #5f6873; font-variant-numeric: tabular-nums; padding-top: .1rem; user-select: none; text-align: right; }
  .cardbody { position: relative; }
  .branch { position: absolute; top: -.1rem; right: 0; opacity: 0; background: none; border: none; color: #6b7178; font-size: .68rem; cursor: pointer; padding: .1rem .3rem; }
  .card:hover .branch { opacity: 1; }
  .branch:hover { color: #8fb8ff; }

  .placement { grid-column: 2; display: flex; flex-wrap: wrap; align-items: center; gap: .4rem; margin: .5rem 0 .2rem; background: #16191e; border: 1px solid #2d3340; border-radius: 8px; padding: .5rem .6rem; }
  .ptitle { font-size: .78rem; color: #9aa0a6; margin-right: .2rem; }

  .addcard { margin-top: .6rem; background: #232830; color: #c8cdd3; }
  .tip { color: #5f6873; font-size: .72rem; margin: .5rem 0 0; }

  .panel { background: #16191e; border: 1px solid #23272e; border-radius: 10px; padding: .9rem 1rem; margin-top: 1.4rem; }
  .panel h3 { margin: 0 0 .5rem; font-size: .95rem; color: #c8cdd3; }
  .panel.backlinks { border-color: #2d3340; }
  .sub { color: #8a9099; font-size: .85rem; margin: .3rem 0; }
  .links { list-style: none; margin: .4rem 0 0; padding: 0; display: flex; flex-direction: column; gap: .4rem; }
  .links li { display: flex; gap: .6rem; align-items: baseline; flex-wrap: wrap; }
  .linkbtn { background: none; border: none; color: #8fb8ff; cursor: pointer; padding: 0; font-size: .9rem; text-align: left; }
  .why { color: #8a9099; font-size: .82rem; }

  input { background: #1a1d22; border: 1px solid #2a2f37; border-radius: 6px; color: #e6e6e6; padding: .4rem .5rem; font: inherit; }
  .title { background: none; }
  button { background: #3b6ea5; color: #fff; border: none; border-radius: 6px; padding: .45rem .8rem; cursor: pointer; font-size: .85rem; }
  button:hover { background: #4880c0; }
  button.ghost { background: #232830; color: #c8cdd3; }
  button.ghost:hover { background: #2c333d; }
  button.full { width: 100%; }
  .row { display: flex; gap: .5rem; align-items: center; margin: .4rem 0; }
  .hint { color: #6b7178; }
  .err { color: #e57373; font-size: .85rem; }
</style>
