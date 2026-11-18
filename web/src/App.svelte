<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { fetchTree, type FileNode } from './lib/api';
  import Canvas from './lib/Canvas.svelte';
  import DiffPanel from './lib/DiffPanel.svelte';
  import { layout } from './lib/layout';

  async function load() {
    const tree = await fetchTree();
    return { title: tree.name, boxes: await layout(tree) };
  }

  // The canvas stays mounted under the diff panel, so closing the panel returns to exactly the same view.
  let openFile = $state<FileNode | undefined>();
</script>

{#await load()}
  <p class="message">Loading changes…</p>
{:then { title, boxes }}
  {#if boxes.length === 0}
    <p class="message">There are no files to review in {title}.</p>
  {:else}
    <SvelteFlowProvider>
      <Canvas {boxes} {title} onopenfile={(file) => (openFile = file)} />
    </SvelteFlowProvider>
    {#if openFile}
      <DiffPanel file={openFile} onclose={() => (openFile = undefined)} />
    {/if}
  {/if}
{:catch error}
  <p class="message error">Could not load the changes: {error.message}</p>
{/await}

<style>
  .message {
    padding: 1rem 2rem;
    color: var(--text-muted);
  }

  .error {
    color: var(--deleted-border);
  }
</style>
