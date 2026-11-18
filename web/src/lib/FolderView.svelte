<script lang="ts">
  import type { FolderBox } from './layout';

  let { data }: { data: { box: FolderBox; open: boolean } } = $props();

  const box = $derived(data.box);
  const side = $derived(Math.min(box.width, box.height));
  // Closed, the name fills the box; open, it fits in the band at the top, above the contents.
  const fontSize = $derived(
    data.open
      ? box.header * 0.55
      : Math.min(
          box.height * 0.22,
          (box.width * 1.6) / Math.max(box.name.length, 4),
        ),
  );
</script>

<div
  class="folder {box.status}"
  class:open={data.open}
  style:width="{box.width}px"
  style:height="{box.height}px"
  style:border-width="{Math.max(1, side * 0.006)}px"
  style:border-radius="{side * 0.03}px"
  title="{box.path}/"
>
  <span
    class="name"
    style:font-size="{fontSize}px"
    style:height={data.open ? `${box.header}px` : '100%'}
    style:padding="0 {box.header * 0.3}px"
  >
    {box.name}
  </span>
</div>

<style>
  .folder {
    box-sizing: border-box;
    border-style: solid;
    border-color: var(--unchanged-border);
    background: var(--unchanged-bg);
    color: var(--text);
  }

  .folder.modified {
    border-color: var(--modified-border);
    background: var(--modified-bg);
  }

  .folder.open {
    background: color-mix(in srgb, var(--unchanged-bg) 40%, var(--canvas));
  }

  .folder.open.modified {
    background: color-mix(in srgb, var(--modified-bg) 40%, var(--canvas));
  }

  .name {
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    font-weight: 600;
    line-height: 1.1;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .open .name {
    justify-content: flex-start;
  }
</style>
