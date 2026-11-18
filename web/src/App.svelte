<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import { fetchTree } from './lib/api';
  import Canvas from './lib/Canvas.svelte';
  import { layout } from './lib/layout';

  async function load() {
    const tree = await fetchTree();
    return { title: tree.name, boxes: await layout(tree) };
  }
</script>

{#await load()}
  <p class="message">Loading changes…</p>
{:then { title, boxes }}
  {#if boxes.length === 0}
    <p class="message">There are no files to review in {title}.</p>
  {:else}
    <SvelteFlowProvider>
      <Canvas {boxes} {title} />
    </SvelteFlowProvider>
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
