<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Elastic } from './elastic.svelte';

  let {
    label,
    keys = true,
    onclose,
    header,
    children,
  }: {
    /** The dialog's accessible name. */
    label: string;
    /** Whether Esc closes the page; off while another page is open over it. */
    keys?: boolean;
    onclose: () => void;
    /** Sticks to the top while the page scrolls. */
    header: Snippet;
    children: Snippet;
  } = $props();

  // A page over the canvas that scrolls like a normal page (ADR 0003). Esc closes it, and so does scrolling on past
  // the end: an elastic arc rises from the bottom of the screen and snaps, closing the page, once it is pulled far
  // enough. Scrolling down is the same wheel direction that zooms out on the canvas, so this mirrors the zoom in that
  // opened the page.

  /** How far, in pixels of wheel movement past the end, the arc must be pulled to snap. */
  const SNAP_DISTANCE = 1200;
  /** The arc's height when it snaps, as a share of the screen's height. */
  const ARC_HEIGHT = 0.3;
  /** How long the page takes to disappear once the arc snaps, in milliseconds. */
  const CLOSE_DURATION = 150;

  let scroller = $state<HTMLElement | undefined>();
  let height = $state(0);
  let closing = $state(false);

  const elastic = new Elastic({
    direction: 'down',
    atEdge: () =>
      scroller !== undefined &&
      scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1,
    pageHeight: () => scroller?.clientHeight ?? 0,
    snapDistance: SNAP_DISTANCE,
    onsnap: () => {
      if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
        onclose();
        return;
      }
      closing = true;
      setTimeout(onclose, CLOSE_DURATION);
    },
  });

  const arc = $derived(elastic.pull.current * ARC_HEIGHT * height);

  $effect(() => () => elastic.destroy());

  function onkeydown(event: KeyboardEvent) {
    if (keys && event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window {onkeydown} />

<div
  class="panel"
  class:closing
  role="dialog"
  aria-modal="true"
  aria-label={label}
  bind:clientHeight={height}
>
  <div class="scroller" bind:this={scroller} onwheel={elastic.onwheel}>
    {@render header()}

    <!-- The content lifts a little as the arc is pulled, like the end of a rubber band. -->
    <div
      class="body"
      style:transform={arc > 0 ? `translateY(${-arc / 4}px)` : undefined}
    >
      {@render children()}
    </div>
  </div>

  {#if arc > 0.5}
    <!-- The canvas pushing up into the page: an arc from the bottom-left corner to the bottom-right corner. -->
    <svg
      class="arc"
      style:height="{arc}px"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <path d="M0,100 Q50,-100 100,100 Z" vector-effect="non-scaling-stroke" />
    </svg>
    <p
      class="hint"
      style:bottom="{arc * 0.3}px"
      style:opacity={elastic.pull.current * 2}
    >
      ↓ Keep scrolling to return to the diagram
    </p>
  {/if}
</div>

<style>
  .panel {
    position: fixed;
    inset: 0;
    z-index: 10;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
  }

  .panel.closing {
    animation: snap 150ms ease-in forwards;
  }

  @keyframes snap {
    to {
      opacity: 0;
      transform: scale(0.96);
    }
  }

  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
  }

  .arc {
    position: absolute;
    bottom: 0;
    left: 0;
    width: 100%;
    pointer-events: none;
  }

  .arc path {
    fill: var(--canvas);
    stroke: var(--unchanged-border);
    stroke-width: 1.5;
  }

  .hint {
    position: absolute;
    left: 0;
    right: 0;
    margin: 0;
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    pointer-events: none;
  }
</style>
