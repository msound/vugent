<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { onMount } from 'svelte';
  import {
    fetchHistory,
    fetchTree,
    type Base,
    type FileNode,
    type FolderNode,
    type Rev,
  } from './lib/api';
  import Canvas from './lib/Canvas.svelte';
  import DetailPage from './lib/DetailPage.svelte';
  import DiffPanel from './lib/DiffPanel.svelte';
  import ReviewHeader from './lib/ReviewHeader.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { listen } from './lib/events';
  import { compare, type Activity } from './lib/activity';
  import { layout, type Box } from './lib/layout';
  import { affects, findFile, serial, touches, type Change } from './lib/live';
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
  let github = $state<string | null>(null);
  let rev = $state<Rev | undefined>();
  let historyError = $state<string | undefined>();

  /** What the canvas shows. It is kept while the next review loads, so the canvas does not flash. */
  let view = $state<
    | {
        rev: Rev;
        tree: FolderNode;
        title: string;
        boxes: Box[];
        summary: Summary;
        /** What changed since the review was last fetched, when it was fetched again. */
        activity?: { nonce: number; changed: Map<string, Activity> };
      }
    | undefined
  >();
  let loading = $state(false);
  let error = $state<string | undefined>();

  // The canvas stays mounted under the diff panel, so closing the panel returns to exactly the same view.
  let openFile = $state<FileNode | undefined>();
  /** The open file has left the review since it was opened, such as an untracked file that was deleted. */
  let fileGone = $state(false);
  /** Counts the changes to the open file, so that the diff panel fetches its diff again. */
  let diffVersion = $state(0);
  /** The server polls for changes rather than watching files (ADR 0008). */
  let polling = $state(false);

  function open(file: FileNode | undefined) {
    openFile = file;
    fileGone = false;
  }
  // The detail page of ADR 0007. It stays open when a row is picked from it, and a diff opens over it.
  let detailOpen = $state(false);

  function onkeydown(event: KeyboardEvent) {
    if (
      event.key !== 'Enter' ||
      detailOpen ||
      openFile !== undefined ||
      selectedRow === undefined ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey ||
      event.shiftKey
    ) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest('input, textarea, select, [contenteditable]')) {
      return;
    }
    event.preventDefault();
    detailOpen = true;
  }

  /** Fetches the sidebar's rows again, keeping the selection while its row is still listed. */
  async function refresh(opened: boolean) {
    try {
      const history = await fetchHistory();
      historyError = undefined;
      rows = rowsOf(history);
      base = history.base;
      github = history.github;
      const wanted = opened ? requested : rev;
      rev = pick(rows, wanted, opened);
    } catch (failure) {
      historyError = (failure as Error).message;
    }
  }

  /** Fetches the sidebar again, once more if asked while it is fetching. */
  const refreshHistory = serial(() => refresh(false));

  /** Counts the times the canvas starts loading a review, so that only the latest load is shown. */
  let loads = 0;

  /** Fetches the tree of `current` and lays it out, showing it unless a newer load started meanwhile. */
  async function load(current: Rev): Promise<FolderNode | undefined> {
    const id = ++loads;
    const tree = await fetchTree(current);
    const boxes = await layout(tree);
    if (id !== loads || rev !== current) {
      return undefined;
    }
    // Fetched again, the boxes whose line counts changed animate (ADR 0008).
    const activity =
      view?.rev === current
        ? { nonce: id, changed: compare(view.tree, tree) }
        : undefined;
    view = {
      rev: current,
      tree,
      title: tree.name,
      boxes,
      summary: summarize(tree),
      activity,
    };
    error = undefined;
    return tree;
  }

  /** Fetches the review on the canvas again, keeping where the canvas is looking and the open diff. */
  const reloadReview = serial(async () => {
    const current = rev;
    if (current === undefined || view?.rev !== current) {
      // The selection is still loading, and gets the latest anyway.
      return;
    }
    try {
      const tree = await load(current);
      if (tree && openFile) {
        const file = findFile(tree, openFile.path);
        if (file) {
          openFile = file;
        }
        fileGone = file === undefined;
      }
    } catch (failure) {
      if (rev === current) {
        error = (failure as Error).message;
      }
    }
  });

  function onchange(change: Change) {
    void refreshHistory();
    if (rev === undefined || !affects(change, rev)) {
      return;
    }
    void reloadReview();
    if (openFile && touches(change, rev, openFile.path)) {
      diffVersion += 1;
    }
  }

  onMount(() => {
    void refresh(true);
    let opened = false;
    return listen({
      onopen: () => {
        // The first time, the page is loading anyway. After a reconnect, changes may have been missed.
        if (opened) {
          void refreshHistory();
          void reloadReview();
          diffVersion += 1;
        }
        opened = true;
      },
      onchange,
      onmode: (value) => (polling = value),
    });
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
    open(undefined);

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
          await load(current);
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

<svelte:window {onkeydown} />

<Sidebar
  {rows}
  selected={rev}
  {loading}
  keys={openFile === undefined && !detailOpen}
  onselect={(next) => (rev = next)}
/>

{#if selectedRow}
  <ReviewHeader
    row={selectedRow}
    {base}
    {summary}
    onopen={() => (detailOpen = true)}
  />
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
          activity={view.activity}
          {polling}
          onopenfile={open}
        />
      </SvelteFlowProvider>
    {/key}
    {#if detailOpen && selectedRow && rows}
      <DetailPage
        row={selectedRow}
        {rows}
        {base}
        {github}
        {summary}
        keys={openFile === undefined}
        onopenfile={open}
        onselect={(next) => (rev = next)}
        onclose={() => (detailOpen = false)}
      />
    {/if}
    {#if openFile}
      <DiffPanel
        file={openFile}
        rev={view.rev}
        version={diffVersion}
        gone={fileGone}
        {context}
        onclose={() => open(undefined)}
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
