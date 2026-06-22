<script lang="ts">
  import { getOutline, type OutlineItem } from "../ipc";

  let {
    docId,
    onGoto,
  }: {
    docId: number;
    onGoto: (page: number) => void;
  } = $props();

  let items = $state<OutlineItem[] | null>(null);
  let failed = $state(false);

  $effect(() => {
    const id = docId;
    items = null;
    failed = false;
    getOutline(id)
      .then((o) => {
        if (id === docId) items = o;
      })
      .catch(() => {
        if (id === docId) failed = true;
      });
  });
</script>

{#snippet node(item: OutlineItem, depth: number)}
  <button
    class="row"
    style="padding-left:{12 + depth * 14}px"
    disabled={item.page === null}
    onclick={() => item.page !== null && onGoto(item.page + 1)}
  >
    {item.title || "(tanpa judul)"}
  </button>
  {#if item.children.length}
    {#each item.children as c, i (i)}
      {@render node(c, depth + 1)}
    {/each}
  {/if}
{/snippet}

<div class="outline">
  {#if items === null && !failed}
    <div class="msg">Memuat…</div>
  {:else if failed}
    <div class="msg">Gagal memuat daftar isi.</div>
  {:else if items && items.length === 0}
    <div class="msg">PDF ini tidak punya daftar isi.</div>
  {:else if items}
    {#each items as it, i (i)}
      {@render node(it, 0)}
    {/each}
  {/if}
</div>

<style>
  .outline {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0 12px;
  }
  .msg {
    padding: 8px 16px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .row {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    padding: 6px 12px 6px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-radius: 6px;
  }
  .row:hover:not(:disabled) {
    background: var(--canvas);
  }
  .row:disabled {
    color: var(--text-muted);
    cursor: default;
  }
</style>
