<script lang="ts">
  import { tick } from 'svelte';
  import type { Rev } from './api';
  import type { Row } from './rows';

  let {
    rows,
    selected,
    loading,
    keys,
    onselect,
  }: {
    rows: Row[] | undefined;
    selected: Rev | undefined;
    /** The selected review is loading. */
    loading: boolean;
    /** Whether J and K move the selection; off while a diff is open. */
    keys: boolean;
    onselect: (rev: Rev) => void;
  } = $props();

  let list = $state<HTMLElement | undefined>();

  function onkeydown(event: KeyboardEvent) {
    if (
      !keys ||
      !rows?.length ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey
    ) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest('input, textarea, select, [contenteditable]')) {
      return;
    }
    const key = event.key.toLowerCase();
    const step = key === 'j' ? 1 : key === 'k' ? -1 : 0;
    if (step === 0) {
      return;
    }
    event.preventDefault();
    const index = rows.findIndex((row) => row.rev === selected);
    const next = Math.min(Math.max(index + step, 0), rows.length - 1);
    if (next !== index) {
      onselect(rows[next].rev);
    }
  }

  // Keep the selected row in view, however it was selected.
  $effect(() => {
    const rev = selected;
    void tick().then(() => {
      list
        ?.querySelector(`[data-rev="${CSS.escape(rev ?? '')}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    });
  });
</script>

<svelte:window {onkeydown} />

<nav class="sidebar" aria-label="What to review" aria-busy={loading}>
  {#if rows === undefined}
    <p class="message">Loading…</p>
  {:else if rows.length === 0}
    <p class="message">Nothing to review.</p>
  {:else}
    <ul bind:this={list}>
      {#each rows as row (row.rev)}
        <li class:indented={row.indented}>
          <button
            type="button"
            class={row.kind}
            class:selected={row.rev === selected}
            aria-current={row.rev === selected ? 'true' : undefined}
            data-rev={row.rev}
            title={row.kind === 'commit'
              ? `${row.label} ${row.detail}`
              : undefined}
            onclick={() => onselect(row.rev)}
          >
            <span class="label">{row.label}</span>
            {#if row.detail !== undefined}
              <span class="detail">{row.detail}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
    <p class="hint">
      {#if loading}
        Loading…
      {:else}
        <kbd>J</kbd> <kbd>K</kbd> to move
      {/if}
    </p>
  {/if}
</nav>

<style>
  .sidebar {
    position: fixed;
    top: 12px;
    left: 12px;
    z-index: 5;
    display: flex;
    flex-direction: column;
    width: var(--sidebar-width);
    max-height: calc(100vh - 80px);
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.12);
    font-size: 0.85rem;
  }

  ul {
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
  }

  li.indented {
    padding-left: 1rem;
  }

  button {
    display: flex;
    gap: 0.5rem;
    width: 100%;
    padding: 0.2rem 0.5rem;
    color: inherit;
    font: inherit;
    text-align: left;
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }

  button:hover {
    background: var(--canvas);
  }

  button.selected {
    background: var(--selected-bg);
    box-shadow: inset 2px 0 0 var(--selected-border);
  }

  .label {
    flex: none;
  }

  .branch .label {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 1 auto;
  }

  .commit .label {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    color: var(--text-muted);
  }

  .detail {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .branch .detail {
    flex: none;
    color: var(--text-muted);
  }

  .message,
  .hint {
    margin: 0;
    padding: 0.4rem 0.75rem;
    color: var(--text-muted);
  }

  .hint {
    border-top: 1px solid var(--unchanged-border);
    font-size: 0.75rem;
  }

  kbd {
    padding: 0 0.3em;
    font: inherit;
    border: 1px solid var(--unchanged-border);
    border-radius: 3px;
  }
</style>
