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
  import type { Box } from './layout';

  let { boxes, title }: { boxes: Box[]; title: string } = $props();

  const nodeTypes: NodeTypes = { folder: FolderView, file: FileView };
  const flow = useSvelteFlow();

  const fitPadding = 0.08;
  let viewport = $state<Viewport>({ x: 0, y: 0, zoom: 1 });

  async function oninit() {
    await flow.fitView({ padding: fitPadding });
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
    if (box.kind === 'folder') {
      void flow.fitBounds(
        { x: box.absX, y: box.absY, width: box.width, height: box.height },
        { duration: 400, padding: fitPadding },
      );
    }
  }
</script>

<div class="canvas">
  <SvelteFlow
    {nodes}
    edges={[]}
    {nodeTypes}
    {oninit}
    onmove={(_, moved) => (viewport = moved)}
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
