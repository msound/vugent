<script lang="ts">
  import { fromColors } from './activity';
  import type { Shown } from './Canvas.svelte';
  import type { FolderBox } from './layout';
  import Sparks from './Sparks.svelte';

  let { data }: { data: { box: FolderBox; open: boolean; activity?: Shown } } =
    $props();

  const box = $derived(data.box);
  const folder = $derived(box.folder);
  const side = $derived(Math.min(box.width, box.height));
  // Color is never the only cue: a changed folder shows the lines added and removed in everything inside it.
  const counts = $derived(
    folder.hue === 'none' ? '' : `+${folder.added} −${folder.removed}`,
  );
  // Closed, the label fills the box; open, it fits in the band at the top, above the contents.
  const fontSize = $derived(
    data.open
      ? box.header * 0.55
      : Math.min(
          box.height * 0.22,
          (box.width * 1.6) /
            Math.max(box.name.length + (counts ? counts.length + 1 : 0), 4),
        ),
  );
  // Only a closed folder shows activity; an open one leaves it to what it contains.
  const activity = $derived(data.activity);
  const from = $derived(fromColors(activity?.from));
</script>

<!-- A new activity draws the box again, which starts its animation over. -->
{#key activity?.nonce}
  <div
    class="folder change {folder.hue} {folder.shade}"
    class:open={data.open}
    class:activity-dust={activity?.effect === 'dust'}
    class:activity-glow={activity?.effect === 'glow'}
    style:width="{box.width}px"
    style:height="{box.height}px"
    style:border-width="{Math.max(1, side * 0.006)}px"
    style:border-radius="{side * 0.03}px"
    style:--from-fill={from.fill}
    style:--from-border={from.border}
    style:--glow-size="{side * 0.08}px"
    title="{box.path}/"
  >
    <span
      class="label"
      style:font-size="{fontSize}px"
      style:height={data.open ? `${box.header}px` : '100%'}
      style:padding="0 {box.header * 0.3}px"
      style:gap="{fontSize * 0.4}px"
    >
      <span class="name">{box.name}</span>
      {#if counts}
        <span class="counts">{counts}</span>
      {/if}
    </span>
    {#if activity?.effect === 'dust'}
      <Sparks width={box.width} height={box.height} />
    {/if}
  </div>
{/key}

<style>
  .folder {
    position: relative;
    box-sizing: border-box;
    border-style: solid;
    border-color: var(--change-border);
    background: var(--change-fill);
    color: var(--text);
  }

  .folder.open {
    background: color-mix(in srgb, var(--change-fill) 40%, var(--canvas));
  }

  .label {
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    line-height: 1.1;
    white-space: nowrap;
  }

  .open .label {
    justify-content: flex-start;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
  }

  .counts {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
</style>
