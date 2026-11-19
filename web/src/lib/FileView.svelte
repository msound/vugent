<script lang="ts">
  import type { FileBox } from './layout';

  let { data }: { data: { box: FileBox } } = $props();

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
</script>

<div
  class="file change {file.hue} {file.shade}"
  class:deleted={file.status === 'deleted'}
  style:width="{box.width}px"
  style:height="{box.height}px"
  title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
>
  <span class="name">{box.name}</span>
  {#if detail}
    <span class="detail">{detail}</span>
  {/if}
</div>

<style>
  .file {
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
