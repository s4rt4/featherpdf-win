<script lang="ts">
  import {
    listPrinters,
    listPapers,
    printDocument,
    renderPageRaw,
    type DocInfo,
    type PaperSize,
    type PrintOpts,
  } from "../ipc";
  import { ChevronLeft, ChevronRight, X } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";

  let {
    doc,
    currentPage,
    onClose,
  }: {
    doc: DocInfo;
    currentPage: number;
    onClose: () => void;
  } = $props();

  let total = $derived(doc.page_count);

  let printers = $state<string[]>([]);
  let printer = $state("");
  let papers = $state<PaperSize[]>([]);
  // Pilihan kertas via key: "default" | "id:<n>" | "custom:<w>x<h>" (0.1 mm).
  let paperKey = $state("default");
  let orientation = $state(0); // 0=default, 1=portrait, 2=landscape
  let copies = $state(1);

  // Ukuran custom yang selalu tersedia (sering tak ada di driver, mis. F4).
  const CUSTOM_PAPERS = [{ label: "F4 / Folio (215 × 330 mm)", w: 2150, h: 3300 }];

  function resolvePaper(): { paper: number; paper_w: number; paper_h: number } {
    if (paperKey.startsWith("id:"))
      return { paper: +paperKey.slice(3), paper_w: 0, paper_h: 0 };
    if (paperKey.startsWith("custom:")) {
      const [w, h] = paperKey.slice(7).split("x").map(Number);
      return { paper: 0, paper_w: w, paper_h: h };
    }
    return { paper: 0, paper_w: 0, paper_h: 0 };
  }
  let rangeMode = $state<"current" | "all" | "custom">("all");
  let customRange = $state("");
  let subset = $state<"all" | "odd" | "even">("all");
  let reverse = $state(false);
  let scaleMode = $state<"fit" | "actual" | "custom">("fit");
  let customScale = $state(100);
  let grayscale = $state(false);
  let autoRotate = $state(true); // putar halaman agar orientasinya pas ke kertas
  let printing = $state(false);
  let progress = $state<{ done: number; total: number } | null>(null);
  let error = $state("");

  // Ukuran kertas dokumen (asumsi seragam; pakai halaman pertama).
  function paperName(wPt: number, hPt: number): string {
    const [a, b] = wPt <= hPt ? [wPt, hPt] : [hPt, wPt];
    const near = (x: number, y: number) => Math.abs(x - y) <= 8;
    if (near(a, 595) && near(b, 842)) return "A4";
    if (near(a, 612) && near(b, 792)) return "Letter";
    if (near(a, 612) && near(b, 1008)) return "Legal";
    if (near(a, 842) && near(b, 1191)) return "A3";
    if (near(a, 420) && near(b, 595)) return "A5";
    if (near(a, 516) && near(b, 728)) return "B5 (JIS)";
    if (near(a, 728) && near(b, 1032)) return "B4 (JIS)";
    if (near(a, 792) && near(b, 1224)) return "Tabloid";
    // F4 / Folio (umum di Indonesia): ~215×330 mm (8.5×13") atau 210×330 mm.
    if ((near(a, 612) || near(a, 595)) && near(b, 936)) return "F4 / Folio";
    return "";
  }
  // Aspek bingkai kertas untuk preview (ikut orientasi). Pakai ukuran custom
  // bila dipilih, selain itu ukuran dokumen sebagai proxy kertas.
  let paperAspect = $derived.by(() => {
    let w = doc.pages[0]?.width ?? 1;
    let h = doc.pages[0]?.height ?? 1;
    if (paperKey.startsWith("custom:")) {
      const [cw, ch] = paperKey.slice(7).split("x").map(Number);
      w = cw;
      h = ch;
    }
    return orientation === 2 ? h / w : w / h; // landscape = lebar
  });

  let docSize = $derived.by(() => {
    const d = doc.pages[0];
    if (!d) return "";
    const mm = (pt: number) => Math.round((pt / 72) * 25.4);
    const name = paperName(d.width, d.height);
    return `${name ? name + " · " : ""}${mm(d.width)} × ${mm(d.height)} mm`;
  });

  let previewIndex = $state(Math.max(0, currentPage - 1));

  // Muat daftar printer.
  $effect(() => {
    listPrinters()
      .then((r) => {
        printers = r.printers;
        printer = r.default ?? r.printers[0] ?? "";
      })
      .catch((e) => (error = String(e)));
  });

  // Muat ukuran kertas saat printer berubah.
  $effect(() => {
    const p = printer;
    if (!p) {
      papers = [];
      return;
    }
    listPapers(p)
      .then((list) => {
        papers = list;
        paperKey = "default";
      })
      .catch(() => (papers = []));
  });

  // Halaman yang akan dicetak (0-based), dari mode range.
  function expandRange(spec: string): number[] {
    const out = new Set<number>();
    for (const part of spec.split(",")) {
      const t = part.trim();
      if (!t) continue;
      const m = t.match(/^(\d+)\s*-\s*(\d+)$/);
      if (m) {
        let a = +m[1];
        let b = +m[2];
        if (a > b) [a, b] = [b, a];
        for (let p = a; p <= b; p++) if (p >= 1 && p <= total) out.add(p - 1);
      } else if (/^\d+$/.test(t)) {
        const p = +t;
        if (p >= 1 && p <= total) out.add(p - 1);
      }
    }
    return [...out].sort((a, b) => a - b);
  }

  let pages = $derived.by<number[]>(() => {
    let base: number[];
    if (rangeMode === "current") base = [Math.max(0, currentPage - 1)];
    else if (rangeMode === "custom") base = expandRange(customRange);
    else base = Array.from({ length: total }, (_, i) => i);
    // subset ganjil/genap (berdasar nomor halaman 1-based)
    if (subset === "odd") base = base.filter((i) => (i + 1) % 2 === 1);
    else if (subset === "even") base = base.filter((i) => (i + 1) % 2 === 0);
    if (reverse) base = [...base].reverse();
    return base;
  });

  // Preview canvas.
  let canvas = $state<HTMLCanvasElement>();
  $effect(() => {
    const idx = previewIndex;
    const cv = canvas;
    if (!cv) return;
    const dim = doc.pages[idx];
    if (!dim) return;
    const scale = 260 / dim.width; // bitmap ~260px lebar
    let cancelled = false;
    renderPageRaw(doc.id, idx, scale)
      .then((bmp) => {
        if (cancelled) {
          bmp.close();
          return;
        }
        cv.width = bmp.width;
        cv.height = bmp.height;
        cv.getContext("2d")?.drawImage(bmp, 0, 0);
        bmp.close();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  async function doPrint() {
    error = "";
    if (!printer) {
      error = "Pilih printer dulu.";
      return;
    }
    if (!pages.length) {
      error = "Tidak ada halaman dalam rentang itu.";
      return;
    }
    const pp = resolvePaper();
    const opts: PrintOpts = {
      printer,
      copies: Math.max(1, Math.floor(copies)),
      pages,
      grayscale,
      scale_mode: scaleMode,
      custom_scale: customScale,
      paper: pp.paper,
      paper_w: pp.paper_w,
      paper_h: pp.paper_h,
      orientation,
      auto_rotate: autoRotate,
    };
    printing = true;
    progress = { done: 0, total: pages.length * opts.copies };
    const unlisten = await listen<[number, number]>("print-progress", (e) => {
      progress = { done: e.payload[0], total: e.payload[1] };
    });
    try {
      await printDocument(doc.id, opts);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      unlisten();
      printing = false;
      progress = null;
    }
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="dialog" role="dialog" aria-label="Cetak">
    <div class="head">
      <span class="title">Cetak</span>
      <button class="x" title="Tutup (Esc)" onclick={onClose}><X size={18} /></button>
    </div>

    <div class="cols">
      <!-- opsi -->
      <div class="opts">
        <label class="row">
          <span class="lbl">Printer</span>
          <select bind:value={printer}>
            {#if !printers.length}<option value="">(tak ada printer)</option>{/if}
            {#each printers as p}<option value={p}>{p}</option>{/each}
          </select>
        </label>

        <label class="row">
          <span class="lbl">Salinan</span>
          <input class="num" type="number" min="1" max="999" bind:value={copies} />
        </label>

        <label class="row">
          <span class="lbl">Kertas</span>
          <select bind:value={paperKey}>
            <option value="default">Default printer</option>
            {#each papers as pp}<option value={"id:" + pp.id}>{pp.name}</option
              >{/each}
            {#each CUSTOM_PAPERS as c}<option value={"custom:" + c.w + "x" + c.h}
                >{c.label}</option
              >{/each}
          </select>
        </label>

        <label class="row">
          <span class="lbl">Orientasi</span>
          <select bind:value={orientation}>
            <option value={0}>Otomatis</option>
            <option value={1}>Tegak (portrait)</option>
            <option value={2}>Mendatar (landscape)</option>
          </select>
        </label>

        <fieldset>
          <legend>Rentang halaman</legend>
          <label class="opt"
            ><input type="radio" value="all" bind:group={rangeMode} /> Semua ({total})</label
          >
          <label class="opt"
            ><input type="radio" value="current" bind:group={rangeMode} /> Halaman ini
            ({currentPage})</label
          >
          <label class="opt">
            <input type="radio" value="custom" bind:group={rangeMode} /> Halaman:
            <input
              class="range-in"
              placeholder="mis. 1,5-9,12"
              bind:value={customRange}
              onfocus={() => (rangeMode = "custom")}
            />
          </label>
          <div class="opt">
            <span style="color:var(--text-muted)">Subset</span>
            <select bind:value={subset} style="flex:1">
              <option value="all">Semua halaman</option>
              <option value="odd">Ganjil saja</option>
              <option value="even">Genap saja</option>
            </select>
          </div>
          <label class="opt">
            <input type="checkbox" bind:checked={reverse} /> Urutan terbalik
          </label>
        </fieldset>

        <fieldset>
          <legend>Skala</legend>
          <div class="doc-size">Ukuran dokumen: {docSize}</div>
          <label class="opt"
            ><input type="radio" value="fit" bind:group={scaleMode} /> Pas ke kertas</label
          >
          <label class="opt"
            ><input type="radio" value="actual" bind:group={scaleMode} /> Ukuran asli</label
          >
          <label class="opt">
            <input type="radio" value="custom" bind:group={scaleMode} /> Custom:
            <input
              class="num"
              type="number"
              min="10"
              max="400"
              bind:value={customScale}
              onfocus={() => (scaleMode = "custom")}
            />%
          </label>
        </fieldset>

        <label class="opt">
          <input type="checkbox" bind:checked={autoRotate} /> Putar otomatis agar pas
          ke kertas
        </label>

        <label class="opt">
          <input type="checkbox" bind:checked={grayscale} /> Cetak grayscale
        </label>

        {#if error}<p class="err">{error}</p>{/if}
      </div>

      <!-- preview -->
      <div class="preview">
        <div class="pv-head">Pratinjau</div>
        <div class="pv-frame">
          <div class="paper" style="aspect-ratio:{paperAspect}">
            <canvas bind:this={canvas} class:gray={grayscale}></canvas>
          </div>
        </div>
        <div class="pv-nav">
          <button
            class="pb"
            disabled={previewIndex <= 0}
            onclick={() => (previewIndex = Math.max(0, previewIndex - 1))}
          >
            <ChevronLeft size={16} />
          </button>
          <span class="pv-lbl">{previewIndex + 1} / {total}</span>
          <button
            class="pb"
            disabled={previewIndex >= total - 1}
            onclick={() => (previewIndex = Math.min(total - 1, previewIndex + 1))}
          >
            <ChevronRight size={16} />
          </button>
        </div>
      </div>
    </div>

    <div class="foot">
      <span class="count">{pages.length} halaman</span>
      <span class="grow"></span>
      <button class="btn" onclick={onClose}>Batal</button>
      <button class="btn primary" disabled={printing || !printer} onclick={doPrint}>
        {#if printing && progress}
          Mencetak {progress.done}/{progress.total}…
        {:else if printing}
          Mencetak…
        {:else}
          Cetak
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dialog {
    width: 720px;
    max-width: calc(100vw - 40px);
    max-height: calc(100vh - 40px);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    font-size: 16px;
    font-weight: 700;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 6px;
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .x:hover {
    background: var(--canvas);
    color: var(--text);
  }
  .cols {
    display: flex;
    gap: 16px;
    padding: 16px;
    min-height: 0;
    overflow: auto;
  }
  .opts {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row .lbl {
    width: 64px;
    color: var(--text-muted);
    font-size: 13px;
  }
  select,
  .num,
  .range-in {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--canvas);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    padding: 5px 8px;
    outline: none;
  }
  select {
    flex: 1;
  }
  .num {
    width: 64px;
  }
  .range-in {
    flex: 1;
    min-width: 90px;
  }
  select:focus,
  .num:focus,
  .range-in:focus {
    border-color: var(--accent);
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  legend {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    padding: 0 4px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .err {
    color: var(--destructive);
    font-size: 13px;
    margin: 0;
  }
  .preview {
    width: 280px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pv-head {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
  }
  .pv-frame {
    flex: 1;
    min-height: 320px;
    background: var(--sunken);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px;
    overflow: hidden;
  }
  .paper {
    max-width: 100%;
    max-height: 340px;
    background: #fff;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.3);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .paper canvas {
    max-width: 100%;
    max-height: 100%;
    display: block;
  }
  .paper canvas.gray {
    filter: grayscale(1);
  }
  .doc-size {
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 2px;
  }
  .pv-nav {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }
  .pv-lbl {
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
  .pb {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--canvas);
    color: var(--text);
    cursor: pointer;
  }
  .pb:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  .count {
    font-size: 13px;
    color: var(--text-muted);
  }
  .grow {
    flex: 1;
  }
  .btn {
    height: 34px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--canvas);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .btn:hover {
    background: var(--surface);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .btn.primary:hover {
    background: var(--accent-hover);
  }
  .btn.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
