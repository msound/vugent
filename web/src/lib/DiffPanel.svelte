<script lang="ts">
  import { Spring } from 'svelte/motion';
  import { fetchDiff, type FileDiff, type FileNode } from './api';

  let { file, onclose }: { file: FileNode; onclose: () => void } = $props();

  let diff = $state<FileDiff | undefined>();
  let error = $state<string | undefined>();

  $effect(() => {
    const path = file.path;
    diff = undefined;
    error = undefined;
    let current = true;
    fetchDiff(path).then(
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

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }

  // Scrolling on past the end of the diff pulls up an elastic arc from the bottom of the screen; once it is pulled
  // far enough, it snaps and the panel closes. Scrolling down in the panel is the same wheel direction that zooms out
  // on the canvas, so this mirrors the zoom in that opened the diff.

  /** Wheel events closer together than this, in milliseconds, belong to the same gesture. */
  const GESTURE_GAP = 150;
  /** How far, in pixels of wheel movement past the end, the arc must be pulled to snap. */
  const SNAP_DISTANCE = 1200;
  /** The arc's height when it snaps, as a share of the screen's height. */
  const ARC_HEIGHT = 0.3;
  /** How long the panel takes to disappear once the arc snaps, in milliseconds. */
  const CLOSE_DURATION = 150;

  let scroller = $state<HTMLElement | undefined>();
  let height = $state(0);

  // How far the arc is pulled, from 0 to 1. It springs back to 0 when the user lets go.
  const pull = new Spring(0, { stiffness: 0.15, damping: 0.5 });
  const arc = $derived(pull.current * ARC_HEIGHT * height);

  let distance = 0;
  let armed = false;
  let lastWheel = -Infinity;
  let release: ReturnType<typeof setTimeout> | undefined;
  let closing = $state(false);

  function onwheel(event: WheelEvent) {
    if (closing || !scroller) {
      return;
    }
    const scale =
      event.deltaMode === WheelEvent.DOM_DELTA_LINE
        ? 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
          ? scroller.clientHeight
          : 1;
    const delta = event.deltaY * scale;

    // Only a gesture that starts at the end pulls the arc. A fast scroll that carries the reader into the end stops
    // there; they must scroll again, on purpose, to pull.
    if (event.timeStamp - lastWheel > GESTURE_GAP) {
      const { scrollTop, scrollHeight, clientHeight } = scroller;
      armed = scrollTop + clientHeight >= scrollHeight - 1;
    }
    lastWheel = event.timeStamp;

    if (!armed || delta <= 0) {
      armed = false;
      letGo();
      return;
    }

    distance += delta;
    if (distance >= SNAP_DISTANCE) {
      snap();
      return;
    }
    // The further the arc is pulled, the harder it pulls back.
    const progress = distance / SNAP_DISTANCE;
    void pull.set(1 - (1 - progress) ** 2, { instant: true });
    clearTimeout(release);
    release = setTimeout(letGo, GESTURE_GAP);
  }

  function letGo() {
    clearTimeout(release);
    distance = 0;
    pull.target = 0;
  }

  function snap() {
    clearTimeout(release);
    void pull.set(1, { instant: true });
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      onclose();
      return;
    }
    closing = true;
    setTimeout(onclose, CLOSE_DURATION);
  }

  $effect(() => () => clearTimeout(release));

  const signs = { added: '+', removed: '-', context: ' ' };
</script>

<svelte:window {onkeydown} />

<div
  class="panel"
  class:closing
  role="dialog"
  aria-modal="true"
  aria-label="Diff of {file.path}"
  bind:clientHeight={height}
>
  <div class="scroller" bind:this={scroller} {onwheel}>
    <header>
      <h2>
        {#if file.oldPath}
          <span class="old">{file.oldPath}</span> →
        {/if}
        {file.path}
      </h2>
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

    <!-- The content lifts a little as the arc is pulled, like the end of a rubber band. -->
    <div
      class="body"
      style:transform={arc > 0 ? `translateY(${-arc / 4}px)` : undefined}
    >
      {#if error}
        <p class="message">Could not load the diff: {error}</p>
      {:else if !diff}
        <p class="message">Loading…</p>
      {:else if diff.submodule}
        <p class="message">
          This is a submodule; vugent does not look inside it.
        </p>
      {:else if diff.binary}
        <p class="message">
          This is a binary file, so there is no diff to show.
        </p>
      {:else if diff.hunks.length === 0}
        <p class="message">
          {#if diff.status === 'renamed'}
            The file was renamed from {diff.oldPath}; its content did not
            change.
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
    </div>
  </div>

  {#if arc > 0.5}
    <!-- The canvas pushing up into the diff: an arc from the bottom-left corner to the bottom-right corner. -->
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
      style:opacity={pull.current * 2}
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
