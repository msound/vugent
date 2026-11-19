<script lang="ts">
  import type { Base } from './api';
  import { Elastic } from './elastic.svelte';
  import type { Row } from './rows';
  import { plural, relativeTime, type Summary } from './summary';

  let {
    row,
    base,
    summary,
    onopen,
  }: {
    row: Row;
    base: Base | null;
    /** The selected review's summary, or undefined while it loads. */
    summary: Summary | undefined;
    /** Opens the detail page. */
    onopen: () => void;
  } = $props();

  // Scrolling up over the header, the wheel direction that zooms in on the canvas, pulls it down like a rubber band;
  // once it is pulled far enough, it snaps and the detail page opens (ADR 0007). It takes a deliberate gesture, so a
  // stray wheel tick over the header does not open it.

  /** How far, in pixels of wheel movement, the header must be pulled to open the page. */
  const SNAP_DISTANCE = 600;
  /** How much taller the header grows when it snaps, in pixels. */
  const STRETCH = 72;

  const elastic = new Elastic({
    direction: 'up',
    atEdge: () => true,
    pageHeight: () => innerHeight,
    snapDistance: SNAP_DISTANCE,
    onsnap: () => {
      onopen();
      elastic.reset();
    },
  });

  $effect(() => () => elastic.destroy());

  const stretch = $derived(elastic.pull.current * STRETCH);

  // Relative dates move on while the page stays open.
  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(timer);
  });

  const title = $derived(
    row.kind === 'commit' ? (row.commit?.subject ?? row.label) : row.label,
  );
</script>

<!-- ADR 0007: two lines that say what the selected review is, the same height for every row. -->
<header
  class="review-header"
  aria-label="Selected review"
  style:height={stretch > 0
    ? `calc(var(--header-height) + ${stretch}px)`
    : undefined}
  onwheel={elastic.onwheel}
>
  <button
    type="button"
    class="open"
    onclick={onopen}
    title="Details (Enter, or scroll up over the header)"
  >
    Details <kbd>⏎</kbd>
  </button>
  <h2 class:branch={row.kind === 'branch'} {title}>{title}</h2>
  <p>
    {#if row.kind === 'commit' && row.commit}
      <span class="sha">{row.commit.shortSha}</span>
      <span class="sep">·</span>
      <span>{row.commit.author}</span>
      <span class="sep">·</span>
      <time datetime={row.commit.date} title={row.commit.date}>
        {relativeTime(row.commit.date, now)}
      </time>
    {:else if row.kind === 'branch'}
      <span>{row.detail}</span>
      {#if base}
        <span>since <code>{base.name}</code> @</span>
        <span class="sha">{base.shortSha}</span>
      {/if}
    {:else if summary}
      <span>{plural(summary.files.length, 'file')} changed</span>
    {:else}
      <span>…</span>
    {/if}
    <span class="sep">·</span>
    {#if summary}
      <span class="counts">
        <span class="plus">+{summary.added}</span>
        <span class="minus">−{summary.removed}</span>
      </span>
    {:else}
      <span class="counts">…</span>
    {/if}
  </p>
  {#if stretch > 0.5}
    <p class="hint" style:opacity={elastic.pull.current * 2}>
      ↑ Keep scrolling to open the details
    </p>
  {/if}
</header>

<style>
  .review-header {
    position: fixed;
    top: 12px;
    left: calc(var(--sidebar-width) + 24px);
    right: 12px;
    z-index: 5;
    box-sizing: border-box;
    height: var(--header-height);
    padding: 0.35rem 0.75rem;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.12);
    overflow: hidden;
  }

  .open {
    float: right;
    margin: 0.4rem 0 0 0.75rem;
    padding: 0.1rem 0.5rem;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.8rem;
    background: none;
    border: 1px solid var(--unchanged-border);
    border-radius: 4px;
    cursor: pointer;
  }

  .open:hover {
    color: var(--text);
    background: var(--canvas);
  }

  kbd {
    font: inherit;
  }

  .hint {
    margin-top: 0.75rem;
    text-align: center;
  }

  h2,
  p {
    margin: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  h2 {
    font-size: 0.95rem;
    font-weight: 600;
  }

  p {
    color: var(--text-muted);
    font-size: 0.8rem;
  }

  p > * + * {
    margin-left: 0.3em;
  }

  .sha,
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  .counts {
    font-variant-numeric: tabular-nums;
  }

  .plus {
    color: var(--added-border);
  }

  .minus {
    color: var(--deleted-border);
  }
</style>
