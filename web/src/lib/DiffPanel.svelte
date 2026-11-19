<script lang="ts">
  import { fetchDiff, type FileDiff, type FileNode, type Rev } from './api';
  import FullScreen from './FullScreen.svelte';

  let {
    file,
    rev,
    context,
    onclose,
  }: {
    file: FileNode;
    rev: Rev;
    /** What the review is, such as a commit's short SHA and subject, shown above the path. */
    context?: string;
    onclose: () => void;
  } = $props();

  let diff = $state<FileDiff | undefined>();
  let error = $state<string | undefined>();

  $effect(() => {
    const path = file.path;
    const review = rev;
    diff = undefined;
    error = undefined;
    let current = true;
    fetchDiff(path, review).then(
      (result) => {
        if (current) diff = result;
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
</script>

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
      <button type="button" onclick={onclose} title="Back to the diagram">
        Esc
      </button>
    </header>
  {/snippet}

  {#if error}
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
      <table>
        {#each diff.hunks as hunk, index (index)}
          <tbody>
            <tr class="hunk">
              <td class="num"></td>
              <td class="num"></td>
              <td class="code" colspan="2">{hunk.header}</td>
            </tr>
            {#each hunk.lines as line, lineIndex (lineIndex)}
              <tr class={line.kind}>
                <td class="num">{line.oldLine ?? ''}</td>
                <td class="num">{line.newLine ?? ''}</td>
                <td class="sign">{signs[line.kind]}</td>
                <td class="code">{line.text}</td>
              </tr>
            {/each}
          </tbody>
        {/each}
      </table>
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

  .added {
    background: var(--added-bg);
  }

  .removed {
    background: var(--deleted-bg);
  }
</style>
