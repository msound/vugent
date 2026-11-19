<script lang="ts">
  import type { Base } from './api';
  import type { Row } from './rows';
  import { plural, relativeTime, type Summary } from './summary';

  let {
    row,
    base,
    summary,
  }: {
    row: Row;
    base: Base | null;
    /** The selected review's summary, or undefined while it loads. */
    summary: Summary | undefined;
  } = $props();

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
<header class="review-header" aria-label="Selected review">
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
