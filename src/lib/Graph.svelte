<script lang="ts" module>
  export type Edge = { from: string; to: string };
  export type RealEdge = { from: string; to: string; why: string };
  export type GraphScore = { hits: RealEdge[]; missed: RealEdge[]; spurious: Edge[] };
</script>

<script lang="ts">
  import type { NodeMeta } from "$lib/api";

  // Props (Svelte 5 runes).
  // - nodes: the notes — always shown (the user knows which papers exist).
  // - drawn: the edges the user reconstructs from memory ($bindable).
  // - result: null while drawing; once set we are in reveal/feedback mode and
  //   drawing is disabled. The real edges are NEVER rendered before this is set,
  //   so the canvas can't leak the structure the user is trying to recall.
  let {
    nodes,
    drawn = $bindable<Edge[]>([]),
    result = null,
  }: {
    nodes: NodeMeta[];
    drawn?: Edge[];
    result?: GraphScore | null;
  } = $props();

  const revealed = $derived(result !== null);

  // --- viewBox + deterministic circle layout (carries no edge information) ---
  const W = 900;
  const H = 600;
  const R = 18; // node radius
  const layout = $derived.by(() => {
    const m: Record<string, { x: number; y: number }> = {};
    const n = nodes.length;
    const cx = W / 2;
    const cy = H / 2;
    const ring = Math.min(W, H) / 2 - 70;
    nodes.forEach((node, i) => {
      // start at the top (-90°) and go clockwise
      const a = (2 * Math.PI * i) / Math.max(n, 1) - Math.PI / 2;
      m[node.id] = { x: cx + ring * Math.cos(a), y: cy + ring * Math.sin(a) };
    });
    return m;
  });

  // Dragged positions override the layout for individual nodes.
  let dragged = $state<Record<string, { x: number; y: number }>>({});
  const pos = (id: string) => dragged[id] ?? layout[id] ?? { x: W / 2, y: H / 2 };
  const titleOf = (id: string) => nodes.find((n) => n.id === id)?.title ?? id;

  // --- selection / drawing state ---
  let pending = $state<string | null>(null); // chosen source awaiting a target
  let svgEl: SVGSVGElement;

  // pointer bookkeeping to tell a click apart from a drag
  let down: { id: string; x: number; y: number; moved: boolean } | null = null;

  function toSvg(e: PointerEvent) {
    const ctm = svgEl.getScreenCTM();
    if (!ctm) return { x: 0, y: 0 };
    const inv = ctm.inverse();
    return { x: e.clientX * inv.a + e.clientY * inv.c + inv.e, y: e.clientX * inv.b + e.clientY * inv.d + inv.f };
  }

  function nodeDown(e: PointerEvent, id: string) {
    if (revealed) return;
    (e.target as Element).setPointerCapture?.(e.pointerId);
    down = { id, x: e.clientX, y: e.clientY, moved: false };
  }
  function nodeMove(e: PointerEvent) {
    if (!down) return;
    if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) down.moved = true;
    if (down.moved) dragged = { ...dragged, [down.id]: toSvg(e) };
  }
  function nodeUp(e: PointerEvent) {
    if (!down) return;
    const { id, moved } = down;
    down = null;
    if (moved) return; // it was a drag, not a click
    selectNode(id);
  }

  function selectNode(id: string) {
    if (revealed) return;
    if (pending === null) {
      pending = id;
    } else if (pending === id) {
      pending = null; // tapped the source again → deselect
    } else {
      addEdge(pending, id);
      pending = null;
    }
  }

  function addEdge(from: string, to: string) {
    if (from === to) return;
    if (drawn.some((e) => e.from === from && e.to === to)) return; // no duplicates
    drawn = [...drawn, { from, to }];
  }
  function removeEdge(edge: Edge) {
    if (revealed) return;
    drawn = drawn.filter((e) => !(e.from === edge.from && e.to === edge.to));
  }

  // Geometry: a directed segment from source edge to target edge, shortened by R
  // at both ends so the arrowhead lands on the node's rim, not its centre.
  function seg(from: string, to: string) {
    const a = pos(from);
    const b = pos(to);
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy) || 1;
    const ux = dx / len;
    const uy = dy / len;
    return { x1: a.x + ux * R, y1: a.y + uy * R, x2: b.x - ux * (R + 6), y2: b.y - uy * (R + 6) };
  }
</script>

<div class="wrap">
  <svg bind:this={svgEl} viewBox="0 0 {W} {H}" onpointermove={nodeMove} onpointerup={nodeUp} role="presentation">
    <defs>
      <marker id="ah-drawn" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,0 L10,5 L0,10 z" fill="#8fb8ff" />
      </marker>
      <marker id="ah-hit" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,0 L10,5 L0,10 z" fill="#8fce8f" />
      </marker>
      <marker id="ah-missed" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,0 L10,5 L0,10 z" fill="#e0a0a0" />
      </marker>
      <marker id="ah-spur" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,0 L10,5 L0,10 z" fill="#c9a85f" />
      </marker>
    </defs>

    {#if !revealed}
      <!-- edges the user has drawn from memory -->
      {#each drawn as e (e.from + ">" + e.to)}
        {@const s = seg(e.from, e.to)}
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="edge drawn" marker-end="url(#ah-drawn)" />
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="edge hit-area"
              onclick={() => removeEdge(e)}
              onkeydown={(ev) => (ev.key === "Enter" || ev.key === " ") && removeEdge(e)}
              role="button" tabindex="0" aria-label="remove edge {titleOf(e.from)} to {titleOf(e.to)}" />
      {/each}
    {:else if result}
      <!-- reveal: the real edges, scored as feedback -->
      {#each result.spurious as e (e.from + ">" + e.to)}
        {@const s = seg(e.from, e.to)}
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="edge spurious" marker-end="url(#ah-spur)" />
      {/each}
      {#each result.missed as e (e.from + ">" + e.to)}
        {@const s = seg(e.from, e.to)}
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="edge missed" marker-end="url(#ah-missed)" />
      {/each}
      {#each result.hits as e (e.from + ">" + e.to)}
        {@const s = seg(e.from, e.to)}
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="edge hit" marker-end="url(#ah-hit)" />
      {/each}
    {/if}

    <!-- nodes: always visible; only the edges are a memory exercise -->
    {#each nodes as n (n.id)}
      {@const p = pos(n.id)}
      <g class="node" class:pending={pending === n.id} class:reveal={revealed}
         onpointerdown={(e) => nodeDown(e, n.id)} role="presentation">
        <circle cx={p.x} cy={p.y} r={R} />
        <text x={p.x} y={p.y + R + 14} text-anchor="middle">{n.title}</text>
      </g>
    {/each}
  </svg>
</div>

<style>
  .wrap { width: 100%; }
  svg { width: 100%; height: auto; display: block; touch-action: none; background: #16191e; border: 1px solid #23272e; border-radius: 10px; }

  .edge { stroke-width: 1.8; fill: none; }
  .edge.drawn { stroke: #8fb8ff; }
  .edge.hit { stroke: #8fce8f; stroke-width: 2.2; }
  .edge.missed { stroke: #e0a0a0; stroke-dasharray: 6 5; }
  .edge.spurious { stroke: #c9a85f; stroke-dasharray: 2 5; opacity: .8; }
  .hit-area { stroke: transparent; stroke-width: 14; cursor: pointer; }

  .node circle { fill: #2a3340; stroke: #4a5566; stroke-width: 1.5; cursor: pointer; }
  .node:not(.reveal):hover circle { fill: #34404f; stroke: #8fb8ff; }
  .node.pending circle { fill: #3b6ea5; stroke: #8fb8ff; stroke-width: 2.5; }
  .node text { fill: #c8cdd3; font-size: 13px; font-family: ui-sans-serif, system-ui, sans-serif; pointer-events: none; user-select: none; }
</style>
