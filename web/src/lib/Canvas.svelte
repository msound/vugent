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
  import FileView from './FileView.svelte';
  import FolderView from './FolderView.svelte';
  import type { FileNode } from './api';
  import { FILE_WIDTH, type Box, type FileBox } from './layout';

  let {
    boxes,
    title,
    onopenfile,
  }: {
    boxes: Box[];
    title: string;
    onopenfile: (file: FileNode) => void;
  } = $props();

  /** A file's diff opens once the file is drawn this much of the screen's width. */
  const FILL = 0.7;

  const nodeTypes: NodeTypes = { folder: FolderView, file: FileView };
  const flow = useSvelteFlow();

  // Leave room on the left for the sidebar: --sidebar-width plus its margin and a gap.
  const fitPadding = {
    top: '4%',
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
      <h1><img src="/logo.svg" alt="" />{title}</h1>
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

  h1 img {
    width: 1.1em;
    height: 1.1em;
  }
</style>
