<script lang="ts" module>
  import type { Activity } from './activity';

  /** Activity being shown on a box; a new nonce starts the animation over. */
  export interface Shown extends Activity {
    nonce: number;
  }
</script>

<script lang="ts">
  import {
    Panel,
    SvelteFlow,
    useSvelteFlow,
    type Node,
    type NodeTypes,
    type Viewport,
  } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { untrack } from 'svelte';
  import { targets } from './activity';
  import FileView from './FileView.svelte';
  import FolderView from './FolderView.svelte';
  import { fetchVersion, type FileNode } from './api';
  import { FILE_WIDTH, type Box, type FileBox } from './layout';

  let {
    boxes,
    title,
    activity,
    polling = false,
    onopenfile,
  }: {
    boxes: Box[];
    title: string;
    /** The boxes whose line counts changed in the last live update, by box id, and a number for that update. */
    activity?: { nonce: number; changed: Map<string, Activity> };
    /** The server polls for changes rather than watching files (ADR 0008). */
    polling?: boolean;
    onopenfile: (file: FileNode) => void;
  } = $props();

  /** A file's diff opens once the file is drawn this much of the screen's width. */
  const FILL = 0.7;

  /** Shown after the title; left out if it cannot be fetched. */
  let version = $state<string>();
  fetchVersion().then(
    (value) => (version = value),
    () => {},
  );

  const nodeTypes: NodeTypes = { folder: FolderView, file: FileView };
  const flow = useSvelteFlow();

  // Leave room on the left for the sidebar (--sidebar-width plus its margin and a gap) and at the top for the header
  // (--header-height plus its margin and a gap).
  const fitPadding = {
    top: '84px',
    right: '4%',
    bottom: '4%',
    left: '320px',
  } as const;
  let viewport = $state<Viewport>({ x: 0, y: 0, zoom: 1 });

  let width = $state(0);
  let height = $state(0);

  // Svelte Flow fits the view itself (the fitView prop). Calling flow.fitView() here instead loops forever inside
  // Svelte Flow for some trees, such as one with only files at the root, and the page freezes.
  function oninit() {
    viewport = flow.getViewport();
  }

  // Every zoom at which a box appears or a folder opens, in ascending order.
  const thresholds = $derived(
    [
      ...new Set(
        boxes.flatMap((box) =>
          box.kind === 'folder'
            ? [box.revealZoom, box.openZoom]
            : [box.revealZoom],
        ),
      ),
    ].sort((a, b) => a - b),
  );

  // The largest threshold the zoom has reached. Nodes only change when it does, not on every frame.
  const reached = $derived.by(() => {
    let low = 0;
    let high = thresholds.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (thresholds[mid] <= viewport.zoom) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low === 0 ? -Infinity : thresholds[low - 1];
  });

  /** How long activity stays on a box, in milliseconds; longer than its animations. */
  const ACTIVITY_DURATION = 1500;

  /** The boxes animating now, by box id. */
  let shown = $state(new Map<string, Shown>());

  // Each live update animates the boxes on screen as they are then. The animation is left alone afterwards, so a box
  // that scrolls into view or opens later does not animate again.
  $effect(() => {
    if (!activity) {
      return;
    }
    const { nonce, changed } = activity;
    shown = untrack(() => {
      const screen = { ...viewport, width, height };
      const found = targets(changed, boxes, reached, screen);
      return new Map(
        [...found].map(([id, change]) => [id, { ...change, nonce }]),
      );
    });
    const timer = setTimeout(() => (shown = new Map()), ACTIVITY_DURATION);
    return () => clearTimeout(timer);
  });

  const nodes = $derived<Node[]>(
    boxes.map((box) => ({
      id: box.id,
      type: box.kind,
      position: { x: box.x, y: box.y },
      parentId: box.parentId,
      width: box.width,
      height: box.height,
      data: {
        box,
        open: box.kind === 'folder' && box.openZoom <= reached,
        activity: shown.get(box.id),
      },
      hidden: box.revealZoom > reached,
      draggable: false,
      selectable: false,
      connectable: false,
    })),
  );

  const files = $derived(
    boxes.filter((box): box is FileBox => box.kind === 'file'),
  );

  // The file whose diff was last opened by zooming in. It does not open again until the user zooms away from it,
  // so that closing the diff returns to the canvas, unchanged.
  let opened: string | undefined;

  function onmove(_: unknown, moved: Viewport) {
    const zoomedIn = moved.zoom > viewport.zoom;
    viewport = moved;
    const file = fillingFile(moved);
    if (!file) {
      opened = undefined;
    } else if (zoomedIn && file.id !== opened) {
      // Only zooming in opens a diff; panning across files that fill the screen does not.
      opened = file.id;
      onopenfile(file.file);
    }
  }

  /** The file at the center of the screen, if it is zoomed in far enough to fill most of the screen. */
  function fillingFile({ x, y, zoom }: Viewport): FileBox | undefined {
    if (width === 0 || FILE_WIDTH * zoom < FILL * width) {
      return undefined;
    }
    const centerX = (width / 2 - x) / zoom;
    const centerY = (height / 2 - y) / zoom;
    return files.find(
      (file) =>
        file.revealZoom <= zoom &&
        centerX >= file.absX &&
        centerX <= file.absX + file.width &&
        centerY >= file.absY &&
        centerY <= file.absY + file.height,
    );
  }

  function onnodeclick({
    node,
    event,
  }: {
    node: Node;
    event: MouseEvent | TouchEvent;
  }) {
    // Svelte Flow has no double-click event for nodes, but a click's detail counts the clicks in a row.
    if (event.detail !== 2) {
      return;
    }
    const box = node.data.box as Box;
    if (box.kind === 'file') {
      onopenfile(box.file);
    } else {
      void flow.fitBounds(
        { x: box.absX, y: box.absY, width: box.width, height: box.height },
        { duration: 400, padding: fitPadding },
      );
    }
  }
</script>

<div class="canvas" bind:clientWidth={width} bind:clientHeight={height}>
  <SvelteFlow
    {nodes}
    edges={[]}
    {nodeTypes}
    {oninit}
    fitView
    fitViewOptions={{ padding: fitPadding }}
    {onmove}
    {onnodeclick}
    minZoom={0.0005}
    maxZoom={12}
    zoomOnDoubleClick={false}
    nodesDraggable={false}
    nodesConnectable={false}
    elementsSelectable={false}
    onlyRenderVisibleElements
  >
    <Panel position="bottom-left">
      <h1>
        <img src="/logo.svg" alt="" />{title}
        {#if version}<span class="version">{version}</span>{/if}
      </h1>
      {#if polling}
        <p
          class="polling"
          title="vugent cannot watch this folder for changes, so it checks every few seconds; its terminal says why."
        >
          Checking for changes every few seconds
        </p>
      {/if}
    </Panel>
  </SvelteFlow>
</div>

<style>
  .canvas {
    position: fixed;
    inset: 0;
  }

  .canvas :global(.svelte-flow) {
    --xy-background-color: var(--canvas);
  }

  h1 {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0;
    padding: 0.2rem 0.6rem;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
  }

  .version {
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--text-muted);
  }

  .polling {
    width: fit-content;
    margin: 0.3rem 0 0;
    padding: 0.1rem 0.6rem;
    font-size: 0.75rem;
    color: var(--text-muted);
    background: var(--bg);
    border: 1px solid var(--unchanged-border);
    border-radius: 6px;
  }

  h1 img {
    width: 1.1em;
    height: 1.1em;
  }
</style>
