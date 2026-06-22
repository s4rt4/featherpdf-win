<script lang="ts">
  import PageView from "./PageView.svelte";
  import type { DocInfo, SearchMatch } from "../ipc";
  import type { Layout } from "../tabs.svelte";

  type PageMatch = { rects: SearchMatch["rects"]; active: boolean };

  let {
    doc,
    zoom,
    cssScale,
    invert = false,
    matches = [],
    activeMatchIndex = -1,
    layout = "continuous",
    page = 1,
    bindContainer = $bindable(),
  }: {
    doc: DocInfo;
    zoom: number;
    cssScale: number;
    invert?: boolean;
    matches?: SearchMatch[];
    activeMatchIndex?: number;
    layout?: Layout;
    page?: number;
    bindContainer?: HTMLDivElement;
  } = $props();

  // Kelompokkan kecocokan per halaman + tandai mana yang sedang aktif.
  let byPage = $derived.by(() => {
    const m = new Map<number, PageMatch[]>();
    matches.forEach((mt, gi) => {
      const arr = m.get(mt.page) ?? [];
      arr.push({ rects: mt.rects, active: gi === activeMatchIndex });
      m.set(mt.page, arr);
    });
    return m;
  });

  // single & facing = PAGINATED (satu halaman/spread per layar, tanpa scroll-snap
  // yang bikin berat). continuous & facing-cont = gulung biasa.
  let paginated = $derived(layout === "single" || layout === "facing");
  let twoCol = $derived(layout === "facing" || layout === "facing-cont");

  // Halaman yang ditampilkan saat paginated.
  let visiblePages = $derived.by(() => {
    const p = Math.min(Math.max(1, page), doc.pages.length) - 1; // 0-based
    if (layout === "facing") {
      const s = Math.floor(p / 2) * 2;
      return s + 1 < doc.pages.length ? [s, s + 1] : [s];
    }
    return [p];
  });

  // Baris untuk mode gulung (1 atau 2 kolom).
  let rows = $derived.by(() => {
    if (!twoCol) return doc.pages.map((_, i) => [i]);
    const r: number[][] = [];
    for (let i = 0; i < doc.pages.length; i += 2) {
      r.push(i + 1 < doc.pages.length ? [i, i + 1] : [i]);
    }
    return r;
  });
</script>

<div class="scroll" class:paginated bind:this={bindContainer}>
  {#if paginated}
    <div class="stage">
      <div class="row">
        {#each visiblePages as i (i)}
          <PageView
            docId={doc.id}
            index={i}
            dim={doc.pages[i]}
            {zoom}
            {cssScale}
            {invert}
            pageMatches={byPage.get(i) ?? []}
          />
        {/each}
      </div>
    </div>
  {:else}
    <div class="pages">
      {#each rows as row (row[0])}
        <div class="row">
          {#each row as i (i)}
            <PageView
              docId={doc.id}
              index={i}
              dim={doc.pages[i]}
              {zoom}
              {cssScale}
              {invert}
              pageMatches={byPage.get(i) ?? []}
            />
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .scroll {
    flex: 1;
    overflow: auto;
    background: var(--sunken);
  }
  .pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 20px;
    min-height: 100%;
  }
  .row {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  /* Paginated: pusatkan via margin:auto (tetap bisa di-scroll bila zoom besar). */
  .scroll.paginated .stage {
    display: flex;
    min-height: 100%;
    padding: 20px;
  }
  .scroll.paginated .row {
    margin: auto;
  }
</style>
