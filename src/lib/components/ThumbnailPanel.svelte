<script lang="ts">
  import { renderPage, type DocInfo } from "../ipc";

  let {
    doc,
    activePage,
    onGoto,
  }: {
    doc: DocInfo;
    activePage: number;
    onGoto: (page: number) => void;
  } = $props();

  const THUMB_W = 124; // lebar thumbnail (px)
  const SCALE = 0.22; // px per point untuk render thumbnail (kecil = cepat)

  let urls = $state<(string | null)[]>([]);

  // Reset & bebaskan saat dokumen berganti.
  $effect(() => {
    void doc.id;
    const arr: (string | null)[] = new Array(doc.page_count).fill(null);
    urls = arr;
    return () => {
      for (const u of arr) if (u) URL.revokeObjectURL(u);
    };
  });

  const rendering = new Set<number>();
  async function renderThumb(i: number) {
    if (urls[i] || rendering.has(i)) return;
    rendering.add(i);
    try {
      const blob = await renderPage(doc.id, i, SCALE);
      urls[i] = URL.createObjectURL(blob);
    } catch (e) {
      console.error("thumbnail", i, e);
    } finally {
      rendering.delete(i);
    }
  }

  // Render saat thumbnail mendekati viewport.
  function inview(node: HTMLElement, i: number) {
    const io = new IntersectionObserver(
      (e) => {
        if (e[0]?.isIntersecting) renderThumb(i);
      },
      { rootMargin: "400px 0px" },
    );
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }
</script>

<div class="thumbs">
  <!-- {#key doc.id}: saat ganti dokumen, recreate semua thumbnail agar
       IntersectionObserver tiap elemen terpasang ulang & langsung fire untuk
       yang sedang terlihat (kalau di-reuse, IO tak trigger ulang). -->
  {#key doc.id}
    {#each doc.pages as dim, i (i)}
      {@const h = THUMB_W * (dim.height / dim.width)}
      <button
        class="thumb"
        class:active={activePage === i + 1}
        use:inview={i}
        onclick={() => onGoto(i + 1)}
      >
        <div class="frame" style="width:{THUMB_W}px;height:{h}px">
          {#if urls[i]}
            <img src={urls[i]} alt={`Halaman ${i + 1}`} draggable="false" />
          {/if}
        </div>
        <span class="n">{i + 1}</span>
      </button>
    {/each}
  {/key}
</div>

<style>
  .thumbs {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 12px 8px;
  }
  .thumb {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
  }
  .frame {
    background: #fff;
    border: 2px solid transparent;
    border-radius: 3px;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  .thumb:hover .frame {
    border-color: var(--text-muted);
  }
  .thumb.active .frame {
    border-color: var(--accent);
  }
  .frame img {
    display: block;
    width: 100%;
    height: 100%;
  }
  .n {
    font-size: 11px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .thumb.active .n {
    color: var(--accent);
    font-weight: 600;
  }
</style>
