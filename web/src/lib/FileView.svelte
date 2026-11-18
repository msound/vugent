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
  class="file {box.status}"
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
    border: 1.5px solid var(--unchanged-border);
    border-radius: 6px;
    background: var(--unchanged-bg);
    color: var(--text);
    line-height: 1.25;
  }

  .added {
    border-color: var(--added-border);
    background: var(--added-bg);
  }

  .modified,
  .renamed {
    border-color: var(--modified-border);
    background: var(--modified-bg);
  }

  .deleted {
    border-color: var(--deleted-border);
    background: var(--deleted-bg);
  }

  .deleted .name {
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
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
