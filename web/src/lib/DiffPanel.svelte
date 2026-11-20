<script lang="ts">
  import {
    fetchDiff,
    type FileDiff,
    type FileNode,
    type Line,
    type Rev,
  } from './api';
  import { SvelteSet } from 'svelte/reactivity';
  import FullScreen from './FullScreen.svelte';
  import {
    foldLines,
    initialLayout,
    rememberLayout,
    splitRows,
    type Block,
    type Layout,
  } from './diffrows';

  let {
    file,
    rev,
    version = 0,
    gone = false,
    context,
    onclose,
  }: {
    file: FileNode;
    rev: Rev;
    /** Changes when the file may have changed, to fetch its diff again. */
    version?: number;
    /** The file is no longer part of the review. */
    gone?: boolean;
    /** What the review is, such as a commit's short SHA and subject, shown above the path. */
    context?: string;
    onclose: () => void;
  } = $props();

  let diff = $state<FileDiff | undefined>();
  let error = $state<string | undefined>();

  // The file's node and the parent's view are replaced whenever the review is fetched again, so only the path and
  // the review's name are followed.
  const path = $derived(file.path);
  const review = $derived(rev);
  /** Which file of which review `diff` shows. */
  let shown: string | undefined;

  $effect(() => {
    const [wanted, wantedRev] = [path, review];
    const key = `${wantedRev}\0${wanted}`;
    void version;
    if (key !== shown) {
      diff = undefined;
      error = undefined;
      // Every diff opens folded. Fetched again after a live update, stretches that still exist stay unfolded.
      unfolded.clear();
      allUnfolded = false;
    }
    if (gone) {
      return;
    }
    let current = true;
    // Fetched again, the diff is replaced in place, so the page keeps its scroll position.
    fetchDiff(wanted, wantedRev).then(
      (result) => {
        if (current) {
          diff = result;
          error = undefined;
          shown = key;
        }
      },
      (failure: Error) => {
        if (current) error = failure.message;
      },
    );
    return () => {
      current = false;
    };
  });

  const signs = { added: '+', removed: '-', context: ' ' };

  /** The folded stretches the user opened, by key. */
  const unfolded = new SvelteSet<string>();
  /** The user unfolded every line. */
  let allUnfolded = $state(false);

  /** What each hunk shows, with folds. */
  const hunks = $derived(
    (diff?.hunks ?? []).map((hunk) => ({
      header: hunk.header,
      blocks: foldLines(hunk.lines, (key) => allUnfolded || unfolded.has(key)),
    })),
  );
  /** Some lines fold, so the toggle that unfolds them all is offered. */
  const foldable = $derived(
    (diff?.hunks ?? []).some((hunk) =>
      foldLines(hunk.lines, () => false).some((block) => block.kind === 'fold'),
    ),
  );

  function switchFolding() {
    allUnfolded = !allUnfolded;
    unfolded.clear();
  }

  let layout = $state<Layout>(initialLayout(window.innerWidth));

  function switchLayout() {
    layout = layout === 'split' ? 'unified' : 'split';
    rememberLayout(layout);
  }

  function onkeydown(event: KeyboardEvent) {
    if (
      event.defaultPrevented ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey
    ) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key !== 's' && (key !== 'a' || !foldable)) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest('input, textarea, select, [contenteditable]')) {
      return;
    }
    event.preventDefault();
    if (key === 's') {
      switchLayout();
    } else {
      switchFolding();
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet fold(block: Block & { kind: 'fold' })}
  <tr class="fold">
    <td colspan="4">
      <button type="button" onclick={() => unfolded.add(block.key)}>
        ⋯ {block.count} unchanged lines
      </button>
    </td>
  </tr>
{/snippet}

{#snippet half(line: Line | undefined, number: number | undefined)}
  {#if line}
    <td class="num {line.kind}">{number}</td>
    <td class="code {line.kind}">{line.text}</td>
  {:else}
    <td class="num blank"></td>
    <td class="code blank"></td>
  {/if}
{/snippet}

<FullScreen label="Diff of {file.path}" {onclose}>
  {#snippet header()}
    <header>
      <div class="titles">
        {#if context}
          <p class="context" title={context}>{context}</p>
        {/if}
        <h2>
          {#if file.oldPath}
            <span class="old">{file.oldPath}</span> →
          {/if}
          {file.path}
        </h2>
      </div>
      <span class="status {file.status}">{file.status}</span>
      {#if !file.binary && !file.submodule && file.status !== 'unchanged'}
        <span class="counts">
          <span class="plus">+{file.added}</span>
          <span class="minus">−{file.removed}</span>
        </span>
      {/if}
      {#if foldable}
        <button
          type="button"
          aria-pressed={allUnfolded}
          onclick={switchFolding}
          title="Show every line, or fold unchanged lines again (A)"
        >
          {allUnfolded ? 'Fold' : 'Unfold all'}
        </button>
      {/if}
      <div class="toggle" role="group" title="Switch layouts (S)">
        {#each [['unified', 'Unified'], ['split', 'Side by side']] as const as [value, name] (value)}
          <button
            type="button"
            aria-pressed={layout === value}
            onclick={() => {
              if (layout !== value) switchLayout();
            }}
          >
            {name}
          </button>
        {/each}
      </div>
      <button type="button" onclick={onclose} title="Back to the diagram">
        Esc
      </button>
    </header>
  {/snippet}

  {#if gone}
    <p class="message">This file is no longer part of the review.</p>
  {:else if error}
    <p class="message">Could not load the diff: {error}</p>
  {:else if !diff}
    <p class="message">Loading…</p>
  {:else if diff.submodule}
    <p class="message">This is a submodule; vugent does not look inside it.</p>
  {:else if diff.binary}
    <p class="message">This is a binary file, so there is no diff to show.</p>
  {:else if diff.hunks.length === 0}
    <p class="message">
      {#if diff.status === 'renamed'}
        The file was renamed from {diff.oldPath}; its content did not change.
      {:else}
        There are no lines to show.
      {/if}
    </p>
  {:else}
    <div class="scroll">
      {#if layout === 'split'}
        <table class="split">
          <colgroup>
            <col class="num-col" />
            <col />
            <col class="num-col" />
            <col />
          </colgroup>
          {#each hunks as hunk, index (index)}
            <tbody>
              {#if !diff.whole}
                <tr class="hunk">
                  <td class="num"></td>
                  <td class="code" colspan="3">{hunk.header}</td>
                </tr>
              {/if}
              {#each hunk.blocks as block, blockIndex (blockIndex)}
                {#if block.kind === 'fold'}
                  {@render fold(block)}
                {:else}
                  {#each splitRows(block.lines) as row, rowIndex (rowIndex)}
                    <tr>
                      {@render half(row.left, row.left?.oldLine)}
                      {@render half(row.right, row.right?.newLine)}
                    </tr>
                  {/each}
                {/if}
              {/each}
            </tbody>
          {/each}
        </table>
      {:else}
        <table>
          {#each hunks as hunk, index (index)}
            <tbody>
              {#if !diff.whole}
                <tr class="hunk">
                  <td class="num"></td>
                  <td class="num"></td>
                  <td class="code" colspan="2">{hunk.header}</td>
                </tr>
              {/if}
              {#each hunk.blocks as block, blockIndex (blockIndex)}
                {#if block.kind === 'fold'}
                  {@render fold(block)}
                {:else}
                  {#each block.lines as line, lineIndex (lineIndex)}
                    <tr class={line.kind}>
                      <td class="num">{line.oldLine ?? ''}</td>
                      <td class="num">{line.newLine ?? ''}</td>
                      <td class="sign">{signs[line.kind]}</td>
                      <td class="code">{line.text}</td>
                    </tr>
                  {/each}
                {/if}
              {/each}
            </tbody>
          {/each}
        </table>
      {/if}
    </div>
  {/if}
</FullScreen>

<style>
  header {
    position: sticky;
    top: 0;
    left: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
    background: var(--bg);
    border-bottom: 1px solid var(--unchanged-border);
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  .context {
    margin: 0;
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.8rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  h2 {
    margin: 0;
    overflow: hidden;
    font-size: 1rem;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .old {
    color: var(--text-muted);
    font-weight: 400;
  }

  .status {
    padding: 0 0.5rem;
    border: 1px solid var(--unchanged-border);
    border-radius: 999px;
    font-size: 0.8rem;
  }

  .status.added {
    border-color: var(--added-border);
    background: var(--added-bg);
  }

  .status.modified,
  .status.renamed {
    border-color: var(--modified-border);
    background: var(--modified-bg);
  }

  .status.deleted {
    border-color: var(--deleted-border);
    background: var(--deleted-bg);
  }

  .counts {
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
  }

  .plus {
    color: var(--added-border);
  }

  .minus {
    color: var(--deleted-border);
  }

  button {
    padding: 0.15rem 0.6rem;
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
    background: var(--unchanged-bg);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .toggle {
    display: flex;
  }

  .toggle button:not(:first-child) {
    border-left: none;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }

  .toggle button:not(:last-child) {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }

  .toggle button[aria-pressed='true'] {
    border-color: var(--selected-border);
    background: var(--selected-bg);
  }

  .toggle button[aria-pressed='true'] + button {
    border-left: 1px solid var(--selected-border);
  }

  .message {
    padding: 1rem;
    color: var(--text-muted);
  }

  .scroll {
    padding-bottom: 2rem;
  }

  table {
    min-width: 100%;
    border-collapse: collapse;
    tab-size: 4;
    font:
      13px/1.5 ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
  }

  td {
    padding: 0 0.5rem;
    vertical-align: top;
  }

  .num {
    width: 1%;
    min-width: 3ch;
    color: var(--text-muted);
    text-align: right;
    user-select: none;
  }

  .sign {
    width: 1ch;
    padding: 0;
    user-select: none;
  }

  .code {
    white-space: pre;
  }

  .hunk td {
    padding-top: 0.25rem;
    padding-bottom: 0.25rem;
    color: var(--text-muted);
    background: var(--canvas);
  }

  .split {
    width: 100%;
    table-layout: fixed;
  }

  .num-col {
    width: 6ch;
  }

  /* Side by side, each half is as wide as the other, so long lines wrap. */
  .split .code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .split td.code {
    border-right: 1px solid var(--unchanged-border);
  }

  .split td.code:last-child {
    border-right: none;
  }

  .blank {
    background: var(--canvas);
  }

  .fold td {
    padding: 0;
    background: var(--canvas);
  }

  .fold button {
    display: block;
    width: 100%;
    padding: 0.1rem 0.5rem;
    border: none;
    border-radius: 0;
    background: none;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
  }

  .fold button:hover {
    background: var(--selected-bg);
    color: var(--text);
  }

  .added {
    background: var(--added-bg);
  }

  .removed {
    background: var(--deleted-bg);
  }
</style>
