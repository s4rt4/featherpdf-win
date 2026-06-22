<script lang="ts">
  import { renderPageRaw, type PageDim, type MatchRect } from "../ipc";

  type PageMatch = { rects: MatchRect[]; active: boolean };

  let {
    docId,
    index,
    dim,
    zoom,
    cssScale,
    invert = false,
    pageMatches = [],
  }: {
    docId: number;
    index: number;
    dim: PageDim;
    zoom: number;
    cssScale: number;
    invert?: boolean;
    pageMatches?: PageMatch[];
  } = $props();

  // points -> CSS px (untuk posisi highlight).
  let factor = $derived(cssScale * zoom);

  let el: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();
  let painted = $state(false); // ada sesuatu (preview/full) tergambar
  let fullNear = $state(false);
  let previewNear = $state(false);
  let renderedScale = 0; // skala full-res terakhir (0 = belum)
  let previewDone = false;
  let rendering = false;
  let previewRendering = false;

  // Dimensi DEVICE pixel integer -> CSS = devicePx/dpr. Tinggi halaman jatuh di
  // batas device-pixel bulat → hilangkan shimmer saat scroll di layar berskala.
  const DPR = window.devicePixelRatio || 1;
  let deviceW = $derived(Math.round(dim.width * cssScale * zoom * DPR));
  let deviceH = $derived(Math.round(dim.height * cssScale * zoom * DPR));
  let displayW = $derived(deviceW / DPR);
  let displayH = $derived(deviceH / DPR);

  const MAX_PX = 4000; // batas dimensi full-res
  const PREVIEW_SCALE = 0.35; // px per point, tetap (tak ikut zoom)

  function drawBitmap(bmp: ImageBitmap) {
    if (!canvas) {
      bmp.close();
      return;
    }
    canvas.width = bmp.width;
    canvas.height = bmp.height;
    const ctx = canvas.getContext("2d");
    if (ctx) ctx.drawImage(bmp, 0, 0);
    bmp.close();
    painted = true;
  }

  // Preview low-res: cepat, tampil instan sebelum full-res siap.
  async function ensurePreview() {
    if (renderedScale !== 0 || previewDone || previewRendering) return;
    previewRendering = true;
    try {
      const bmp = await renderPageRaw(docId, index, PREVIEW_SCALE);
      if (renderedScale === 0) {
        drawBitmap(bmp);
        previewDone = true;
      } else {
        bmp.close();
      }
    } catch (e) {
      console.error("preview halaman", index, e);
    } finally {
      previewRendering = false;
    }
  }

  // Full-res sesuai zoom & dpr (gambar 1:1 device pixel → tajam).
  async function ensureFull() {
    if (!fullNear) return;
    const maxDimPt = Math.max(dim.width, dim.height);
    let scale = cssScale * zoom * DPR;
    scale = Math.min(scale, MAX_PX / maxDimPt);

    if (rendering || Math.abs(scale - renderedScale) < 0.01) return;
    rendering = true;
    try {
      const bmp = await renderPageRaw(docId, index, scale);
      drawBitmap(bmp);
      renderedScale = scale;
    } catch (e) {
      console.error("render halaman", index, e);
    } finally {
      rendering = false;
    }
  }

  // Observer preview: jangkauan SANGAT jauh (murah, aman).
  $effect(() => {
    const io = new IntersectionObserver(
      (e) => (previewNear = e[0]?.isIntersecting ?? false),
      { rootMargin: "7000px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  });

  // Observer full-res: jangkauan sedang.
  $effect(() => {
    const io = new IntersectionObserver(
      (e) => (fullNear = e[0]?.isIntersecting ?? false),
      { rootMargin: "3000px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  });

  $effect(() => {
    if (previewNear) ensurePreview();
  });

  // Picu full-res. Halaman baru: langsung. Re-render karena zoom: di-debounce.
  let renderTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void zoom;
    void fullNear;
    if (!fullNear) return;
    if (renderedScale === 0) {
      ensureFull();
      return;
    }
    clearTimeout(renderTimer);
    renderTimer = setTimeout(ensureFull, 90);
    return () => clearTimeout(renderTimer);
  });
</script>

<div
  bind:this={el}
  class="page"
  class:invert
  style="width:{displayW}px;height:{displayH}px"
  data-page={index + 1}
>
  <canvas bind:this={canvas} class="cv"></canvas>

  {#if !painted}
    <div class="placeholder">
      <span class="spinner"></span>
      <span class="pnum">{index + 1}</span>
    </div>
  {/if}

  {#if pageMatches.length && painted}
    <div class="highlights">
      {#each pageMatches as m}
        {#each m.rects as r}
          <span
            class="hl"
            class:active={m.active}
            style="left:{r.x * factor}px;top:{r.y * factor}px;width:{r.w *
              factor}px;height:{r.h * factor}px"
          ></span>
        {/each}
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    position: relative;
    background: #fff;
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.4);
    flex: none;
  }
  .cv {
    display: block;
    width: 100%;
    height: 100%;
    /* Layer GPU sendiri: saat scroll, compositor menggeser layer (sampling
       konsisten) alih-alih raster ulang CPU tiap frame → tekan jitter/shimmer. */
    transform: translateZ(0);
  }
  .page.invert {
    background: #111;
  }
  .page.invert .cv {
    filter: invert(1) hue-rotate(180deg);
  }
  .page.invert .placeholder {
    background: #1b1c20;
  }
  .highlights {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .hl {
    position: absolute;
    background: rgba(255, 213, 0, 0.4);
    mix-blend-mode: multiply;
    border-radius: 1px;
  }
  .hl.active {
    background: rgba(255, 145, 0, 0.55);
    outline: 1.5px solid rgba(229, 120, 0, 0.9);
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: #fbfbfc;
  }
  .pnum {
    color: #c2c5cc;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border: 2px solid #e4e6ea;
    border-top-color: #b3b7bf;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
