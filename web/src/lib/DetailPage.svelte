<script lang="ts">
  import {
    fetchCommit,
    type Base,
    type CommitDetails,
    type FileNode,
    type Rev,
    type Signature,
  } from './api';
  import FullScreen from './FullScreen.svelte';
  import type { Row } from './rows';
  import { plural, relativeTime, type Summary } from './summary';

  let {
    row,
    rows,
    base,
    github,
    summary,
    keys,
    onopenfile,
    onselect,
    onclose,
  }: {
    row: Row;
    /** Every row of the sidebar, for the branch's list of commits. */
    rows: Row[];
    base: Base | null;
    /** The repository's GitHub address, to link issue references to. */
    github: string | null;
    /** The review's summary, or undefined while it loads. */
    summary: Summary | undefined;
    /** Whether Esc closes the page; off while a diff is open over it. */
    keys: boolean;
    onopenfile: (file: FileNode) => void;
    onselect: (rev: Rev) => void;
    onclose: () => void;
  } = $props();

  let details = $state<CommitDetails | undefined>();
  let error = $state<string | undefined>();

  $effect(() => {
    const sha = row.commit?.sha;
    details = undefined;
    error = undefined;
    if (sha === undefined) {
      return;
    }
    let current = true;
    fetchCommit(sha).then(
      (result) => {
        if (current) details = result;
      },
      (failure: Error) => {
        if (current) error = failure.message;
      },
    );
    return () => {
      current = false;
    };
  });

  const title = $derived(row.commit?.subject ?? row.label);
  const commits = $derived(rows.filter((other) => other.kind === 'commit'));

  /** What uncommitted work compares, from ADR 0005 and ADR 0006. */
  const comparisons: Partial<Record<Row['kind'], string>> = {
    worktree: 'The working tree, untracked files included, against HEAD.',
    unstaged:
      'The working tree, untracked files included, against the index: what is not staged yet.',
    index: 'The index against HEAD: what the next commit will contain.',
  };

  const now = Date.now();

  function sameSignature(a: Signature, b: Signature) {
    return a.name === b.name && a.email === b.email && a.date === b.date;
  }

  function formatDate(date: string) {
    return new Date(date).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    });
  }

  /** Splits a trailer's value into text and issue references such as `#25`, which link to GitHub. */
  function references(value: string): { text: string; issue?: string }[] {
    return value
      .split(/(#\d+)/)
      .filter((text) => text !== '')
      .map((text) =>
        github && /^#\d+$/.test(text)
          ? { text, issue: `${github}/issues/${text.slice(1)}` }
          : { text },
      );
  }

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      // The clipboard can be refused; the SHA is still there to select.
    }
  }

  $effect(() => () => clearTimeout(copiedTimer));
</script>

<FullScreen label="Details of {title}" {keys} {onclose}>
  {#snippet header()}
    <header>
      <h2 {title}>{title}</h2>
      <button type="button" onclick={onclose} title="Back to the diagram">
        Esc
      </button>
    </header>
  {/snippet}

  <div class="page">
    {#if row.kind === 'commit'}
      {#if error}
        <p class="muted">Could not load the commit: {error}</p>
      {:else if !details}
        <p class="muted">Loading…</p>
      {:else}
        {#if details.body}
          <pre class="message">{details.body}</pre>
        {/if}
        {#if details.trailers.length > 0}
          <ul class="trailers" aria-label="Trailers">
            {#each details.trailers as trailer, index (index)}
              <li>
                <span class="key">{trailer.key}</span>
                {#each references(trailer.value) as part, partIndex (partIndex)}
                  {#if part.issue}
                    <a href={part.issue} target="_blank" rel="noreferrer">
                      {part.text}
                    </a>
                  {:else}
                    {part.text}
                  {/if}
                {/each}
              </li>
            {/each}
          </ul>
        {/if}
        <dl>
          <dt>Commit</dt>
          <dd>
            <code>{details.sha}</code>
            <button
              type="button"
              class="small"
              onclick={() => copy(details!.sha)}
            >
              {copied ? 'Copied' : 'Copy'}
            </button>
          </dd>
          <dt>Author</dt>
          <dd>
            {details.author.name}
            <span class="muted">&lt;{details.author.email}&gt;</span>
            ·
            <time datetime={details.author.date}>
              {formatDate(details.author.date)}
            </time>
            <span class="muted">
              ({relativeTime(details.author.date, now)})
            </span>
          </dd>
          {#if !sameSignature(details.author, details.committer)}
            <dt>Committer</dt>
            <dd>
              {details.committer.name}
              <span class="muted">&lt;{details.committer.email}&gt;</span>
              ·
              <time datetime={details.committer.date}>
                {formatDate(details.committer.date)}
              </time>
              <span class="muted">
                ({relativeTime(details.committer.date, now)})
              </span>
            </dd>
          {/if}
        </dl>
      {/if}
    {:else if row.kind === 'branch'}
      <p class="muted">
        Everything since the branch left
        {#if base}<code>{base.name}</code> at
          <code>{base.shortSha}</code>{:else}the main branch{/if}, uncommitted
        work included.
      </p>
      <h3>{plural(commits.length, 'commit')}</h3>
      <ul class="list">
        {#each commits as commit (commit.rev)}
          <li>
            <button type="button" onclick={() => onselect(commit.rev)}>
              <code class="muted">{commit.label}</code>
              <span class="grow">{commit.detail}</span>
              {#if commit.commit}
                <span class="muted">
                  {commit.commit.author} · {relativeTime(
                    commit.commit.date,
                    now,
                  )}
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{comparisons[row.kind]}</p>
    {/if}

    {#if summary}
      <h3>
        {plural(summary.files.length, 'file')} changed
        <span class="counts">
          <span class="plus">+{summary.added}</span>
          <span class="minus">−{summary.removed}</span>
        </span>
      </h3>
      <ul class="list">
        {#each summary.files as file (file.path)}
          <li>
            <button type="button" onclick={() => onopenfile(file)}>
              <span class="status {file.status}">{file.status}</span>
              <span class="grow path">
                {#if file.oldPath}<span class="muted">
                    {file.oldPath} →
                  </span>{/if}
                {file.path}
              </span>
              {#if !file.binary && !file.submodule}
                <span class="counts">
                  <span class="plus">+{file.added}</span>
                  <span class="minus">−{file.removed}</span>
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">Loading the changed files…</p>
    {/if}
  </div>
</FullScreen>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
    background: var(--bg);
    border-bottom: 1px solid var(--unchanged-border);
  }

  h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 1rem;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .page {
    max-width: 80ch;
    margin: 0 auto;
    padding: 1rem 1rem 4rem;
  }

  .message {
    margin: 0 0 1rem;
    font:
      14px/1.5 ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .trailers {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0 0 1rem;
    padding: 0;
    list-style: none;
    font-size: 0.85rem;
  }

  .trailers li {
    padding: 0.1rem 0.6rem;
    border: 1px solid var(--unchanged-border);
    border-radius: 999px;
    background: var(--canvas);
  }

  .key {
    color: var(--text-muted);
  }

  .key::after {
    content: ':';
  }

  a {
    color: var(--selected-border);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.3rem 1rem;
    margin: 0 0 1rem;
    font-size: 0.9rem;
  }

  dt {
    color: var(--text-muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.9em;
  }

  .muted {
    color: var(--text-muted);
  }

  h3 {
    margin: 1.5rem 0 0.5rem;
    font-size: 0.95rem;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
  }

  .list li + li {
    border-top: 1px solid var(--unchanged-border);
  }

  .list button {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    width: 100%;
    padding: 0.35rem 0.75rem;
    color: inherit;
    font: inherit;
    font-size: 0.9rem;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
  }

  .list button:hover {
    background: var(--canvas);
  }

  .grow {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
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

  button.small {
    margin-left: 0.4rem;
    padding: 0 0.5rem;
    font-size: 0.8rem;
  }

  .status {
    flex: none;
    width: 5.5rem;
    padding: 0 0.5rem;
    border: 1px solid var(--unchanged-border);
    border-radius: 999px;
    font-size: 0.75rem;
    text-align: center;
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
    flex: none;
    font-size: 0.85rem;
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  .plus {
    color: var(--added-border);
  }

  .minus {
    color: var(--deleted-border);
  }
</style>
