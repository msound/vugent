<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { onMount } from 'svelte';
  import {
    fetchHistory,
    fetchTree,
    type Base,
    type FileNode,
    type Rev,
  } from './lib/api';
  import Canvas from './lib/Canvas.svelte';
  import DiffPanel from './lib/DiffPanel.svelte';
  import ReviewHeader from './lib/ReviewHeader.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { layout, type Box } from './lib/layout';
  import { pick, rowsOf, type Row } from './lib/rows';
  import { summarize, type Summary } from './lib/summary';

  /** How long, in milliseconds, the selection must rest before the canvas loads it, so that holding J does not
   * load every row on the way. */
  const SETTLE = 150;

  /** The review named in the URL when the page opened, read before the selection rewrites the URL. */
  const requested =
    new URLSearchParams(location.search).get('rev') ?? undefined;

  let rows = $state<Row[] | undefined>();
  let base = $state<Base | null>(null);
  let rev = $state<Rev | undefined>();
  let historyError = $state<string | undefined>();

  /** What the canvas shows. It is kept while the next review loads, so the canvas does not flash. */
  let view = $state<
    { rev: Rev; title: string; boxes: Box[]; summary: Summary } | undefined
  >();
  let loading = $state(false);
  let error = $state<string | undefined>();

  // The canvas stays mounted under the diff panel, so closing the panel returns to exactly the same view.
  let openFile = $state<FileNode | undefined>();

  /** Fetches the sidebar's rows again, keeping the selection while its row is still listed. */
  async function refresh(opened: boolean) {
    try {
      const history = await fetchHistory();
      historyError = undefined;
      rows = rowsOf(history);
      base = history.base;
      const wanted = opened ? requested : rev;
      rev = pick(rows, wanted, opened);
    } catch (failure) {
      historyError = (failure as Error).message;
    }
  }

  onMount(() => {
    void refresh(true);
  });

  const selectedRow = $derived(rows?.find((row) => row.rev === rev));

  /** The summary of the selected review, once it has loaded. */
  const summary = $derived(view?.rev === rev ? view?.summary : undefined);

  /** What the diff panel says about the review, above the file's path. */
  const context = $derived(
    selectedRow?.commit
      ? `${selectedRow.commit.shortSha} ${selectedRow.commit.subject}`
      : selectedRow?.label,
  );

  let first = true;

  $effect(() => {
    const current = rev;
    openFile = undefined;

    const url = new URL(location.href);
    if (current === undefined) {
      url.searchParams.delete('rev');
    } else {
      url.searchParams.set('rev', current);
    }
    history.replaceState(history.state, '', url);

    if (current === undefined) {
      view = undefined;
      loading = false;
      return;
    }
    let live = true;
    loading = true;
    const timer = setTimeout(
      async () => {
        try {
          const tree = await fetchTree(current);
          const boxes = await layout(tree);
          if (live) {
            view = {
              rev: current,
              title: tree.name,
              boxes,
              summary: summarize(tree),
            };
            error = undefined;
          }
        } catch (failure) {
          if (live) {
            error = (failure as Error).message;
          }
        } finally {
          if (live) {
            loading = false;
          }
        }
      },
      first ? 0 : SETTLE,
    );
    first = false;
    return () => {
      live = false;
      clearTimeout(timer);
    };
  });
</script>

<svelte:window onfocus={() => void refresh(false)} />

<Sidebar
  {rows}
  selected={rev}
  {loading}
  keys={openFile === undefined}
  onselect={(next) => (rev = next)}
/>

{#if selectedRow}
  <ReviewHeader row={selectedRow} {base} {summary} />
{/if}

{#if historyError}
  <p class="message error">Could not load the history: {historyError}</p>
{:else if error}
  <p class="message error">Could not load the changes: {error}</p>
{:else if view}
  {#if view.boxes.length === 0}
    <p class="message">There are no files to review in {view.title}.</p>
  {:else}
    {#key view.rev}
      <SvelteFlowProvider>
        <Canvas
          boxes={view.boxes}
          title={view.title}
          onopenfile={(file) => (openFile = file)}
        />
      </SvelteFlowProvider>
    {/key}
    {#if openFile}
      <DiffPanel
        file={openFile}
        rev={view.rev}
        {context}
        onclose={() => (openFile = undefined)}
      />
    {/if}
  {/if}
{:else if rev !== undefined}
  <p class="message">Loading changes…</p>
{/if}

<style>
  .message {
    padding: calc(var(--header-height) + 1.5rem) 2rem 1rem
      calc(var(--sidebar-width) + 2rem);
    color: var(--text-muted);
  }

  .error {
    color: var(--deleted-border);
  }
</style>
