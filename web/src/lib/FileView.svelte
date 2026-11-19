<script lang="ts">
  import { fromColors } from './activity';
  import type { FileBox } from './layout';
  import Sparks from './Sparks.svelte';
  import type { Shown } from './Canvas.svelte';

  let { data }: { data: { box: FileBox; activity?: Shown } } = $props();

  const box = $derived(data.box);
  const file = $derived(box.file);
  const detail = $derived.by(() => {
    if (file.submodule) {
      return 'submodule';
    }
    if (file.binary) {
      return 'binary';
    }
    if (file.status === 'unchanged') {
      return '';
    }
    return `+${file.added} −${file.removed}`;
  });
  const activity = $derived(data.activity);
  const from = $derived(fromColors(activity?.from));
</script>

<!-- A new activity draws the box again, which starts its animation over. -->
{#key activity?.nonce}
  <div
    class="file change {file.hue} {file.shade}"
    class:deleted={file.status === 'deleted'}
    class:activity-dust={activity?.effect === 'dust'}
    class:activity-glow={activity?.effect === 'glow'}
    style:width="{box.width}px"
    style:height="{box.height}px"
    style:--from-fill={from.fill}
    style:--from-border={from.border}
    style:--glow-size="18px"
    title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
  >
    <span class="name">{box.name}</span>
    {#if detail}
      <span class="detail">{detail}</span>
    {/if}
    {#if activity?.effect === 'dust'}
      <Sparks width={box.width} height={box.height} />
    {/if}
  </div>
{/key}

<style>
  .file {
    position: relative;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 0 10px;
    border: 1.5px solid var(--change-border);
    border-radius: 6px;
    background: var(--change-fill);
    color: var(--text);
    line-height: 1.25;
  }

  .file.deleted .name {
    text-decoration: line-through;
  }

  .name {
    overflow: hidden;
    font-size: 17px;
    font-weight: 500;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .detail {
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
</style>
