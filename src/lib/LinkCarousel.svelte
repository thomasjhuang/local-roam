<!--
  LinkCarousel (#20b) — flip-to-recall. One face-down card per note (minus the open
  one), shuffled. A card flips only when the user recalls its exact title (the parent
  resolves through api.resolveLink — exact-or-nothing, so this can never degrade into
  a readable pick-list). A flipped card is dragged onto the open note to start an
  edge, landing in the tweet-capped justify step (#20a).
-->
<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";

  /** Drag payload type for a flipped card. */
  export const CARD_MIME = "application/x-local-roam-card";

  /**
   * Session-only flip scoreboard: ids whose titles the user has retrieved from
   * memory this session. Module state — it survives closing and reopening the
   * link panel, but dies with the app. Deliberately never persisted: next
   * session the cards are face-down again and must be re-earned.
   */
  export const flipped = new SvelteSet<string>();

  // Stable-for-the-session shuffle: each card keeps one random sort key so the
  // strip doesn't reshuffle every time the panel opens. Plain (non-reactive)
  // Map — only consulted while the deck recomputes.
  const shuffleKey = new Map<string, number>();
</script>

<script lang="ts">
  import type { NodeMeta } from "$lib/api";

  let {
    notes,
    excludeId,
    onDragChange,
  }: {
    notes: NodeMeta[];
    excludeId: string;
    /** Fires with the card on dragstart and null on dragend, so the parent can show its drop zone. */
    onDragChange?: (card: NodeMeta | null) => void;
  } = $props();

  const deck = $derived.by(() => {
    const cards = notes.filter((n) => n.id !== excludeId);
    for (const c of cards) if (!shuffleKey.has(c.id)) shuffleKey.set(c.id, Math.random());
    return [...cards].sort((a, b) => shuffleKey.get(a.id)! - shuffleKey.get(b.id)!);
  });
  const retrieved = $derived(deck.filter((c) => flipped.has(c.id)).length);

  function startDrag(e: DragEvent, card: NodeMeta) {
    if (!e.dataTransfer) return;
    e.dataTransfer.setData(CARD_MIME, card.id);
    e.dataTransfer.effectAllowed = "link";
    onDragChange?.(card);
  }
</script>

<div class="carousel">
  <div class="strip" role="list">
    {#each deck as card (card.id)}
      {#if flipped.has(card.id)}
        <div
          class="card up"
          role="listitem"
          draggable="true"
          title="Drag onto the note to link"
          ondragstart={(e) => startDrag(e, card)}
          ondragend={() => onDragChange?.(null)}
        >
          {card.title}
        </div>
      {:else}
        <div class="card down" role="listitem" aria-label="a face-down card">?</div>
      {/if}
    {/each}
    {#if !deck.length}
      <p class="none">No other notes to link to yet.</p>
    {/if}
  </div>
  <p class="score">{retrieved} of {deck.length} retrieved this session — flips reset next launch</p>
</div>

<style>
  .carousel { margin-top: .6rem; }
  .strip { display: flex; gap: .45rem; overflow-x: auto; padding: .3rem 0 .5rem; }
  .card {
    flex: 0 0 auto; width: 6.2rem; height: 3.6rem; border-radius: 8px;
    display: flex; align-items: center; justify-content: center; text-align: center;
    font-size: .72rem; padding: .3rem; box-sizing: border-box; user-select: none;
  }
  .card.down {
    background: repeating-linear-gradient(135deg, #1b1f26, #1b1f26 6px, #20252d 6px, #20252d 12px);
    border: 1px solid #2a2f37; color: #4a5057; font-size: 1.1rem;
  }
  .card.up {
    background: #1a2230; border: 1px solid #3b6ea5; color: #8fb8ff;
    cursor: grab; overflow: hidden;
  }
  .card.up:active { cursor: grabbing; }
  .score { color: #6b7178; font-size: .72rem; margin: 0; }
  .none { color: #6b7178; font-size: .8rem; margin: 0; }
</style>
