<script lang="ts">
  import Viewer from "./lib/components/Viewer.svelte";
  import ThumbnailPanel from "./lib/components/ThumbnailPanel.svelte";
  import OutlinePanel from "./lib/components/OutlinePanel.svelte";
  import PrintDialog from "./lib/components/PrintDialog.svelte";
  import { untrack } from "svelte";
  import { pickPdfs, searchDocument, type SearchMatch } from "./lib/ipc";
  import { tabStore, type FitMode, type Layout } from "./lib/tabs.svelte";
  import logoUrl from "./assets/feather-logo.svg";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    FolderOpen,
    Search,
    ChevronUp,
    ChevronDown,
    ZoomIn,
    ZoomOut,
    MoveHorizontal,
    Maximize,
    Maximize2,
    Minimize2,
    Contrast,
    Sun,
    Moon,
    Plus,
    X,
    Minus,
    Square,
    Images,
    List,
    RectangleVertical,
    Rows2,
    Columns2,
    Grid2x2,
    Printer,
  } from "@lucide/svelte";

  const CSS_SCALE = 96 / 72; // points -> CSS px pada 100%
  const win = getCurrentWindow();

  let loading = $state(false);
  let errorMsg = $state("");
  let container = $state<HTMLDivElement>();

  // Panel kiri (thumbnail/outline) — isi menyusul di Fase 3.
  let panel = $state<"none" | "thumbs" | "outline">("none");

  // Preferensi tampilan (dipersistensi). Default gelap sesuai semangat Feather.
  type Theme = "dark" | "light";
  let theme = $state<Theme>(
    localStorage.getItem("feather:theme") === "light" ? "light" : "dark",
  );
  let invert = $state(localStorage.getItem("feather:invert") === "1");

  let active = $derived(tabStore.active);

  // Pencarian teks (per dokumen aktif).
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchMatches = $state<SearchMatch[]>([]);
  let searchIndex = $state(-1);
  let searching = $state(false);
  let searchInput = $state<HTMLInputElement>();

  $effect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("feather:theme", theme);
  });
  $effect(() => {
    localStorage.setItem("feather:invert", invert ? "1" : "0");
  });

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
  }

  // Muat daftar path PDF berurutan (await per file) agar tak ada lonjakan
  // permintaan ke backend. Hanya file pertama yang difokuskan & di-fit.
  async function openPaths(paths: string[]) {
    const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
    if (!pdfs.length) return;
    loading = true;
    errorMsg = "";
    try {
      for (let i = 0; i < pdfs.length; i++) {
        await tabStore.open(pdfs[i], i === 0);
        if (i === 0) requestAnimationFrame(() => applyFit("width"));
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleOpen() {
    const paths = await pickPdfs();
    if (paths.length) await openPaths(paths);
  }

  function closeTab(id: number, e: MouseEvent) {
    e.stopPropagation();
    tabStore.close(id);
  }

  // Ubah zoom sambil mempertahankan posisi baca (titik di atas viewport tetap),
  // dengan menskala scrollTop seproporsional perubahan zoom. Mulus untuk slider,
  // ctrl+wheel, tombol, maupun fit.
  function applyZoom(z: number, mode: FitMode) {
    if (!active) return;
    const z0 = active.zoom;
    const z1 = clampZoom(z);
    const top0 = container?.scrollTop ?? 0;
    active.zoom = z1;
    active.fitMode = mode;
    const ratio = z0 > 0 ? z1 / z0 : 1;
    requestAnimationFrame(() => {
      if (container) container.scrollTop = top0 * ratio;
    });
  }

  function colsOf(l: Layout) {
    return l === "facing" || l === "facing-cont" ? 2 : 1;
  }

  // Hitung nilai zoom untuk fit lebar/halaman, memperhitungkan jumlah kolom
  // (mode facing butuh muat 2 halaman berdampingan).
  function fitZoom(mode: "width" | "page", cols: number): number | null {
    if (!active || !container) return null;
    const first = active.doc.pages[0];
    if (!first) return null;
    const pad = 52;
    const availW = container.clientWidth - pad;
    const availH = container.clientHeight - pad;
    const perW = cols === 2 ? (availW - 16) / 2 : availW;
    const fitW = perW / (first.width * CSS_SCALE);
    if (mode === "width") return clampZoom(fitW);
    const fitH = availH / (first.height * CSS_SCALE);
    return clampZoom(Math.min(fitW, fitH));
  }

  function applyFit(mode: "width" | "page") {
    if (!active) return;
    const z = fitZoom(mode, colsOf(active.layout));
    if (z != null) applyZoom(z, mode);
  }

  function setLayout(l: Layout) {
    if (!active) return;
    const page = active.page;
    active.layout = l;
    requestAnimationFrame(() => {
      // Re-fit kalau sedang mode fit (jumlah kolom berubah), lalu kembali ke
      // halaman yang sama (perubahan 1↔2 kolom mengubah tinggi total).
      if (active && (active.fitMode === "width" || active.fitMode === "page")) {
        const z = fitZoom(active.fitMode, colsOf(l));
        if (z != null) active.zoom = z;
      }
      requestAnimationFrame(() => goToPage(page));
    });
  }

  function clampZoom(z: number) {
    return Math.min(6, Math.max(0.1, z));
  }

  function setZoom(z: number) {
    applyZoom(z, "custom");
  }

  // Input manual halaman & zoom.
  function gotoPageInput(v: string) {
    const n = parseInt(v, 10);
    if (!isNaN(n)) goToPage(n);
  }
  function setZoomPercent(v: string) {
    const n = parseFloat(v);
    if (!isNaN(n)) setZoom(n / 100);
  }

  // Dropdown preset zoom.
  let zoomMenuOpen = $state(false);
  const ZOOM_PRESETS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];
  function applyPreset(z: number) {
    zoomMenuOpen = false;
    setZoom(z);
  }

  let printOpen = $state(false); // dialog cetak
  let dragging = $state(false); // overlay drag-drop file
  let confirmClose = $state(false); // dialog konfirmasi tutup aplikasi

  // Fullscreen (mode baca imersif: chrome disembunyikan).
  let fullscreen = $state(false);
  async function toggleFullscreen() {
    fullscreen = !fullscreen;
    try {
      if (fullscreen) {
        // Un-maximize dulu: window yang sedang maximized lalu di-fullscreen-kan
        // bisa menyisakan strip hitam (area native tak ter-cover webview).
        if (await win.isMaximized()) await win.unmaximize();
        await win.setFullscreen(true);
      } else {
        await win.setFullscreen(false);
      }
    } catch (e) {
      console.error("fullscreen", e);
    }
  }

  function paginatedLayout(l: Layout) {
    return l === "single" || l === "facing";
  }

  function goToPage(n: number) {
    if (!active) return;
    const clamped = Math.min(active.doc.page_count, Math.max(1, n));
    if (paginatedLayout(active.layout)) {
      // Mode paginated: cukup ganti halaman aktif (Viewer menampilkannya) &
      // reset scroll ke atas halaman baru.
      active.page = clamped;
      if (container)
        requestAnimationFrame(() => {
          if (container) container.scrollTop = 0;
        });
      return;
    }
    if (!container) return;
    const el = container.querySelector<HTMLElement>(`[data-page="${clamped}"]`);
    if (!el) return;
    // Lompat instan via offset (scrollIntoView smooth tak andal untuk lompatan
    // jauh ratusan halaman di WebView2 — sering cuma bergerak sedikit).
    const crect = container.getBoundingClientRect();
    const erect = el.getBoundingClientRect();
    container.scrollTop += erect.top - crect.top;
  }

  // --- Pencarian teks ---
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearchInput() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(runSearch, 250);
  }

  async function runSearch() {
    const q = searchQuery.trim();
    if (!active || !q) {
      searchMatches = [];
      searchIndex = -1;
      return;
    }
    const docId = active.doc.id;
    searching = true;
    try {
      const res = await searchDocument(docId, q);
      if (active?.doc.id !== docId) return; // tab berganti saat menunggu
      searchMatches = res;
      searchIndex = res.length ? 0 : -1;
      if (res.length) goToMatch(0);
    } catch (e) {
      console.error("search", e);
      searchMatches = [];
      searchIndex = -1;
    } finally {
      searching = false;
    }
  }

  function goToMatch(i: number) {
    if (!active || !container || !searchMatches.length) return;
    const len = searchMatches.length;
    const n = ((i % len) + len) % len; // wrap
    searchIndex = n;
    const m = searchMatches[n];
    const pageEl = container.querySelector<HTMLElement>(
      `[data-page="${m.page + 1}"]`,
    );
    if (!pageEl) return;
    pageEl.scrollIntoView({ block: "start" });
    const r = m.rects[0];
    if (r) container.scrollTop += r.y * active.zoom * CSS_SCALE - 90;
  }

  function openSearch() {
    searchOpen = true;
    requestAnimationFrame(() => searchInput?.focus());
  }
  function closeSearch() {
    searchOpen = false;
    searchMatches = [];
    searchIndex = -1;
    searchQuery = "";
  }

  // Bersihkan hasil pencarian saat ganti tab (match milik dokumen sebelumnya).
  $effect(() => {
    void active?.id;
    searchMatches = [];
    searchIndex = -1;
  });

  // Simpan posisi scroll (O(1), murah). Halaman aktif dihitung terpisah via
  // IntersectionObserver (lihat $effect) — JANGAN baca getBoundingClientRect
  // per-frame di sini: pada dokumen ratusan halaman itu memicu ratusan
  // forced-reflow tiap frame → scroll bergetar/jank.
  // Simpan posisi scroll dengan DEBOUNCE (bukan tiap frame) — menulis state
  // reaktif tiap frame scroll memicu flush Svelte berulang → bisa bikin jitter.
  let scrollSaveTimer: ReturnType<typeof setTimeout> | undefined;
  let lastFlip = 0; // cooldown page-flip via roda mouse (mode paginated)
  function onScroll() {
    clearTimeout(scrollSaveTimer);
    scrollSaveTimer = setTimeout(() => {
      if (container && active) active.scrollTop = container.scrollTop;
    }, 200);
  }

  // (Re)pasang listener scroll & re-fit saat container/tab/layout berubah.
  // Dependensi `layout`: ganti mode menyusun ulang elemen halaman, jadi
  // IntersectionObserver perlu dipasang ulang ke elemen yang baru.
  $effect(() => {
    void active?.id;
    void active?.layout;
    const c = container;
    if (!c) return;
    c.addEventListener("scroll", onScroll, { passive: true });
    // Ctrl + roda mouse = zoom. Di mode paginated, roda di tepi atas/bawah =
    // balik halaman (page-flip). Listener non-passive agar bisa preventDefault.
    const onWheel = (e: WheelEvent) => {
      if (!active) return;
      if (e.ctrlKey) {
        e.preventDefault();
        setZoom(active.zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
        return;
      }
      if (paginatedLayout(active.layout)) {
        const atTop = c.scrollTop <= 0;
        const atBottom = c.scrollTop + c.clientHeight >= c.scrollHeight - 1;
        const step = active.layout === "facing" ? 2 : 1;
        const now = Date.now();
        if (now - lastFlip <= 250) return; // cooldown anti loncat banyak
        if (e.deltaY > 0 && atBottom && active.page < active.doc.page_count) {
          e.preventDefault();
          lastFlip = now;
          goToPage(active.page + step);
        } else if (e.deltaY < 0 && atTop && active.page > 1) {
          e.preventDefault();
          lastFlip = now;
          goToPage(active.page - step);
        }
      }
    };
    c.addEventListener("wheel", onWheel, { passive: false });
    // Pulihkan posisi scroll tab ini. Tinggi tiap halaman sudah deterministik
    // dari dims+zoom (tak menunggu gambar), jadi scrollTop bisa langsung diset.
    // untrack: JANGAN jadikan scrollTop dependency effect — kalau tidak, simpan
    // scrollTop (debounce) memicu effect re-run → scroll "menghentak" balik.
    const savedTop = untrack(() => active?.scrollTop ?? 0);
    if (savedTop > 0) requestAnimationFrame(() => (c.scrollTop = savedTop));

    // Halaman aktif via IntersectionObserver — HANYA untuk mode gulung. Di mode
    // paginated, halaman aktif ditentukan navigasi (jangan ditimpa observer).
    let pageObserver: IntersectionObserver | undefined;
    if (!paginatedLayout(active?.layout ?? "continuous")) {
      const band = new Set<number>();
      pageObserver = new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            const p = Number((e.target as HTMLElement).dataset.page);
            if (e.isIntersecting) band.add(p);
            else band.delete(p);
          }
          if (band.size && active) active.page = Math.min(...band);
        },
        { root: c, rootMargin: "0px 0px -80% 0px" },
      );
      for (const el of c.querySelectorAll("[data-page]")) pageObserver.observe(el);
    }
    // Tunda applyFit ke frame berikutnya: mengubah zoom di dalam callback RO
    // memicu RO lagi di frame yang sama ("loop completed with undelivered
    // notifications"). rAF memecah loop itu.
    let fitRaf = 0;
    const ro = new ResizeObserver(() => {
      if (fitRaf) return;
      fitRaf = requestAnimationFrame(() => {
        fitRaf = 0;
        if (active && (active.fitMode === "width" || active.fitMode === "page"))
          applyFit(active.fitMode);
      });
    });
    ro.observe(c);
    return () => {
      c.removeEventListener("scroll", onScroll);
      c.removeEventListener("wheel", onWheel);
      pageObserver?.disconnect();
      ro.disconnect();
      if (fitRaf) cancelAnimationFrame(fitRaf);
    };
  });

  // Pintasan keyboard.
  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey && (e.key === "o" || e.key === "O")) {
        e.preventDefault();
        handleOpen();
      } else if (e.ctrlKey && (e.key === "w" || e.key === "W")) {
        e.preventDefault();
        if (active) tabStore.close(active.id);
      } else if (e.ctrlKey && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        if (active) setZoom(active.zoom * 1.2);
      } else if (e.ctrlKey && e.key === "-") {
        e.preventDefault();
        if (active) setZoom(active.zoom / 1.2);
      } else if (e.ctrlKey && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        if (active) openSearch();
      } else if (e.ctrlKey && (e.key === "p" || e.key === "P")) {
        e.preventDefault();
        if (active) printOpen = true;
      } else if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen();
      } else if (e.key === "Escape") {
        if (printOpen) printOpen = false;
        else if (searchOpen) closeSearch();
        else if (fullscreen) toggleFullscreen();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Drag & drop file PDF ke jendela.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const t = e.payload.type;
        if (t === "enter" || t === "over") dragging = true;
        else if (t === "drop") {
          dragging = false;
          openPaths(e.payload.paths);
        } else dragging = false; // leave / cancel
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  // Konfirmasi sebelum menutup aplikasi bila ada >1 tab terbuka.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    win
      .onCloseRequested((event) => {
        if (tabStore.tabs.length > 1) {
          event.preventDefault();
          confirmClose = true;
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  // Tutup dropdown preset zoom saat klik di luar.
  $effect(() => {
    if (!zoomMenuOpen) return;
    const close = (e: PointerEvent) => {
      if (!(e.target as HTMLElement).closest(".zoom-field")) zoomMenuOpen = false;
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  });
</script>

<div class="win" class:fullscreen>
  <!-- title bar -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="brand">
      <img src={logoUrl} alt="" width="18" height="18" />
      <span>Feather PDF</span>
    </div>
    <div class="wctl">
      <button class="wbtn" title="Minimalkan" onclick={() => win.minimize()}>
        <Minus size={15} />
      </button>
      <button
        class="wbtn"
        title="Maksimalkan"
        onclick={() => win.toggleMaximize()}
      >
        <Square size={13} />
      </button>
      <button class="wbtn close" title="Tutup" onclick={() => win.close()}>
        <X size={16} />
      </button>
    </div>
  </div>

  <!-- tab strip -->
  <div class="tabs">
    {#each tabStore.tabs as t (t.id)}
      <div
        class="tab"
        class:active={t.id === tabStore.activeId}
        onclick={() => tabStore.activate(t.id)}
        role="tab"
        tabindex="0"
        onkeydown={(e) => e.key === "Enter" && tabStore.activate(t.id)}
        title={t.path}
      >
        <span class="nm">{t.name}</span>
        <button
          class="x"
          title="Tutup tab"
          onclick={(e) => closeTab(t.id, e)}
        >
          <X size={16} strokeWidth={2.5} />
        </button>
      </div>
    {/each}
    <button class="tab add" title="Buka PDF (Ctrl+O)" onclick={handleOpen}>
      <Plus size={16} />
    </button>
    <div class="spacer" data-tauri-drag-region></div>
    <div class="right">
      <button
        class="rbtn-top"
        class:on={searchOpen}
        title="Cari (Ctrl+F)"
        onclick={() => (searchOpen ? closeSearch() : openSearch())}
      >
        <Search size={16} />
      </button>
      <button
        class="rbtn-top"
        title={theme === "dark" ? "Tema terang" : "Tema gelap"}
        onclick={toggleTheme}
      >
        {#if theme === "dark"}<Sun size={16} />{:else}<Moon size={16} />{/if}
      </button>
    </div>
  </div>

  <!-- command toolbar (hanya saat ada dokumen) -->
  {#if active}
    <div class="toolbar">
      <button class="tbtn" title="Buka PDF (Ctrl+O)" onclick={handleOpen}>
        <FolderOpen size={17} />
      </button>
      <button class="tbtn" title="Cetak (Ctrl+P)" onclick={() => (printOpen = true)}>
        <Printer size={17} />
      </button>
      <span class="sep"></span>
      <button
        class="tbtn"
        title="Halaman sebelumnya"
        onclick={() => goToPage(active.page - 1)}
      >
        <ChevronUp size={17} />
      </button>
      <input
        class="num-in"
        value={active.page}
        onchange={(e) => gotoPageInput(e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            gotoPageInput(e.currentTarget.value);
            e.currentTarget.blur();
          }
        }}
        aria-label="Nomor halaman"
      />
      <span class="of">/ {active.doc.page_count}</span>
      <button
        class="tbtn"
        title="Halaman berikutnya"
        onclick={() => goToPage(active.page + 1)}
      >
        <ChevronDown size={17} />
      </button>
      <span class="sep"></span>
      <button
        class="tbtn"
        class:on={active.layout === "single"}
        title="Satu halaman"
        onclick={() => setLayout("single")}
      >
        <RectangleVertical size={17} />
      </button>
      <button
        class="tbtn"
        class:on={active.layout === "continuous"}
        title="Menggulung (continuous)"
        onclick={() => setLayout("continuous")}
      >
        <Rows2 size={17} />
      </button>
      <button
        class="tbtn"
        class:on={active.layout === "facing"}
        title="Dua halaman (facing)"
        onclick={() => setLayout("facing")}
      >
        <Columns2 size={17} />
      </button>
      <button
        class="tbtn"
        class:on={active.layout === "facing-cont"}
        title="Dua halaman menggulung"
        onclick={() => setLayout("facing-cont")}
      >
        <Grid2x2 size={17} />
      </button>
      <span class="sep"></span>
      <button
        class="tbtn"
        title="Perkecil (Ctrl+−)"
        onclick={() => setZoom(active.zoom / 1.2)}
      >
        <ZoomOut size={17} />
      </button>
      <input
        class="zoom-slider"
        type="range"
        min="10"
        max="600"
        step="1"
        value={Math.round(active.zoom * 100)}
        oninput={(e) => setZoom(+e.currentTarget.value / 100)}
        aria-label="Zoom"
      />
      <button
        class="tbtn"
        title="Perbesar (Ctrl++)"
        onclick={() => setZoom(active.zoom * 1.2)}
      >
        <ZoomIn size={17} />
      </button>
      <div class="zoom-field">
        <input
          class="num-in zoom-in"
          value={Math.round(active.zoom * 100) + "%"}
          onchange={(e) => setZoomPercent(e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              setZoomPercent(e.currentTarget.value);
              e.currentTarget.blur();
            }
          }}
          aria-label="Persen zoom"
        />
        <button
          class="caret"
          title="Preset zoom"
          onclick={() => (zoomMenuOpen = !zoomMenuOpen)}
        >
          <ChevronDown size={13} />
        </button>
        {#if zoomMenuOpen}
          <div class="zoom-menu">
            {#each ZOOM_PRESETS as z}
              <button onclick={() => applyPreset(z)}>{Math.round(z * 100)}%</button
              >
            {/each}
            <div class="zm-sep"></div>
            <button
              onclick={() => {
                zoomMenuOpen = false;
                applyFit("width");
              }}>Fit lebar</button
            >
            <button
              onclick={() => {
                zoomMenuOpen = false;
                applyFit("page");
              }}>Fit halaman</button
            >
          </div>
        {/if}
      </div>
      <button
        class="tbtn"
        class:on={active.fitMode === "width"}
        title="Sesuaikan lebar"
        onclick={() => applyFit("width")}
      >
        <MoveHorizontal size={17} />
      </button>
      <button
        class="tbtn"
        class:on={active.fitMode === "page"}
        title="Sesuaikan halaman"
        onclick={() => applyFit("page")}
      >
        <Maximize size={17} />
      </button>
      <span class="sep"></span>
      <button
        class="tbtn"
        class:on={invert}
        title="Balik warna isi PDF (mode malam)"
        onclick={() => (invert = !invert)}
      >
        <Contrast size={17} />
      </button>
      <span class="grow"></span>
      <button
        class="tbtn"
        title={fullscreen ? "Keluar layar penuh (Esc)" : "Layar penuh (F11)"}
        onclick={toggleFullscreen}
      >
        {#if fullscreen}<Minimize2 size={17} />{:else}<Maximize2 size={17} />{/if}
      </button>
    </div>
  {/if}

  <!-- body -->
  <div class="body" class:has-doc={!!active}>
    {#if loading}
      <div class="center muted">Memuat…</div>
    {:else if errorMsg}
      <div class="center">
        <p class="err">{errorMsg}</p>
        <button class="open-cta" onclick={handleOpen}>Coba lagi</button>
      </div>
    {:else if active}
      <!-- left rail -->
      <div class="rail">
        <button
          class="rbtn"
          class:on={panel === "thumbs"}
          title="Thumbnail"
          onclick={() => (panel = panel === "thumbs" ? "none" : "thumbs")}
        >
          <Images size={19} />
        </button>
        <button
          class="rbtn"
          class:on={panel === "outline"}
          title="Daftar isi"
          onclick={() => (panel = panel === "outline" ? "none" : "outline")}
        >
          <List size={19} />
        </button>
      </div>

      {#if panel !== "none"}
        <div class="panel">
          <div class="panel-head">
            {panel === "thumbs" ? "Thumbnail" : "Daftar isi"}
          </div>
          {#if panel === "thumbs"}
            <ThumbnailPanel
              doc={active.doc}
              activePage={active.page}
              onGoto={goToPage}
            />
          {:else}
            <OutlinePanel docId={active.doc.id} onGoto={goToPage} />
          {/if}
        </div>
      {/if}

      <!-- viewport -->
      <div class="view-wrap">
        {#if searchOpen}
          <div class="findbar">
            <Search size={15} />
            <input
              bind:this={searchInput}
              bind:value={searchQuery}
              oninput={onSearchInput}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (searchMatches.length)
                    goToMatch(searchIndex + (e.shiftKey ? -1 : 1));
                  else runSearch();
                }
              }}
              placeholder="Cari di dokumen…"
              spellcheck="false"
            />
            <span class="count">
              {#if searching}…{:else if searchMatches.length}{searchIndex +
                  1}/{searchMatches.length}{:else if searchQuery.trim()}0{/if}
            </span>
            <button
              class="fb"
              title="Sebelumnya (Shift+Enter)"
              disabled={!searchMatches.length}
              onclick={() => goToMatch(searchIndex - 1)}
            >
              <ChevronUp size={15} />
            </button>
            <button
              class="fb"
              title="Berikutnya (Enter)"
              disabled={!searchMatches.length}
              onclick={() => goToMatch(searchIndex + 1)}
            >
              <ChevronDown size={15} />
            </button>
            <button class="fb" title="Tutup (Esc)" onclick={closeSearch}>
              <X size={15} />
            </button>
          </div>
        {/if}
        {#key active.id}
          <Viewer
            doc={active.doc}
            zoom={active.zoom}
            {invert}
            cssScale={CSS_SCALE}
            layout={active.layout}
            page={active.page}
            matches={searchMatches}
            activeMatchIndex={searchIndex}
            bind:bindContainer={container}
          />
        {/key}

        <!-- floating pill nav -->
        <div class="pill">
          <button class="pb" title="Sebelumnya" onclick={() => goToPage(active.page - 1)}>
            <ChevronUp size={15} />
          </button>
          <span class="lbl">{active.page} / {active.doc.page_count}</span>
          <button class="pb" title="Berikutnya" onclick={() => goToPage(active.page + 1)}>
            <ChevronDown size={15} />
          </button>
          <span class="vsep"></span>
          <button class="pb" title="Perkecil" onclick={() => setZoom(active.zoom / 1.2)}>
            <ZoomOut size={15} />
          </button>
          <span class="lbl">{Math.round(active.zoom * 100)}%</span>
          <button class="pb" title="Perbesar" onclick={() => setZoom(active.zoom * 1.2)}>
            <ZoomIn size={15} />
          </button>
          {#if fullscreen}
            <span class="vsep"></span>
            <button
              class="pb"
              title="Keluar layar penuh (Esc)"
              onclick={toggleFullscreen}
            >
              <Minimize2 size={15} />
            </button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="center empty">
        <img class="empty-logo" src={logoUrl} alt="" width="84" height="84" />
        <h1>Feather PDF</h1>
        <p class="muted">Buka file PDF untuk mulai membaca.</p>
        <button class="open-cta" onclick={handleOpen}>
          <FolderOpen size={18} />
          <span>Buka PDF</span>
        </button>
      </div>
    {/if}
  </div>

  {#if printOpen && active}
    <PrintDialog
      doc={active.doc}
      currentPage={active.page}
      onClose={() => (printOpen = false)}
    />
  {/if}

  {#if dragging}
    <div class="drop-overlay">
      <div class="drop-card">
        <FolderOpen size={36} />
        <span>Lepas file PDF di sini</span>
      </div>
    </div>
  {/if}

  {#if confirmClose}
    <div class="modal-backdrop">
      <div class="confirm">
        <div class="c-title">Tutup Feather PDF?</div>
        <p class="c-msg">
          Ada {tabStore.tabs.length} dokumen terbuka. Yakin ingin menutup aplikasi?
        </p>
        <div class="c-actions">
          <button class="c-btn" onclick={() => (confirmClose = false)}>Batal</button>
          <button class="c-btn danger" onclick={() => win.destroy()}>Tutup</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--canvas);
  }
  /* Mode layar penuh: sembunyikan semua chrome, sisakan viewport + pill nav. */
  .win.fullscreen .titlebar,
  .win.fullscreen .tabs,
  .win.fullscreen .toolbar,
  .win.fullscreen .rail {
    display: none;
  }

  /* overlay drag-drop */
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: color-mix(in srgb, var(--accent) 18%, rgba(0, 0, 0, 0.35));
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }
  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px 48px;
    background: var(--surface);
    color: var(--accent);
    border: 2px dashed var(--accent);
    border-radius: 16px;
    font-size: 15px;
    font-weight: 600;
  }

  /* modal konfirmasi tutup */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .confirm {
    width: 360px;
    max-width: calc(100vw - 40px);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.4);
    padding: 20px;
  }
  .c-title {
    font-size: 16px;
    font-weight: 700;
    margin-bottom: 8px;
  }
  .c-msg {
    font-size: 13px;
    color: var(--text-muted);
    margin: 0 0 18px;
    line-height: 1.5;
  }
  .c-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .c-btn {
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
  .c-btn:hover {
    background: var(--bg-elevated);
  }
  .c-btn.danger {
    background: var(--destructive);
    border-color: var(--destructive);
    color: #fff;
    font-weight: 600;
  }

  /* title bar */
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding: 0 0 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 13px;
    pointer-events: none;
  }
  .brand img {
    display: block;
  }
  .wctl {
    display: flex;
    height: 100%;
  }
  .wbtn {
    width: 44px;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .wbtn:hover {
    background: var(--canvas);
    color: var(--text);
  }
  .wbtn.close:hover {
    background: var(--destructive);
    color: #fff;
  }

  /* tab strip */
  .tabs {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 6px 8px 0;
    background: var(--canvas);
    flex: none;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 7px;
    max-width: 220px;
    padding: 8px 9px 8px 13px;
    font-size: 13px;
    color: var(--text-muted);
    border-radius: 8px 8px 0 0;
    cursor: default;
    position: relative;
    white-space: nowrap;
  }
  .tab:hover {
    color: var(--text);
    background: rgba(127, 127, 127, 0.12);
  }
  .tab .nm {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab.active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    margin-bottom: -1px;
    padding-bottom: 9px;
  }
  .tab.active::before {
    content: "";
    position: absolute;
    top: 0;
    left: 8px;
    right: 8px;
    height: 2px;
    background: var(--accent);
    border-radius: 2px;
  }
  .tab .x {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 5px;
    opacity: 0;
    color: var(--text-muted);
    cursor: pointer;
    flex: none;
  }
  .tab.active .x,
  .tab:hover .x {
    opacity: 1;
  }
  .tab .x:hover {
    background: var(--border);
    color: var(--text);
  }
  .tab.add {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 8px 10px;
    border-radius: 8px;
    cursor: pointer;
  }
  .tab.add:hover {
    color: var(--text);
    background: rgba(127, 127, 127, 0.12);
  }
  .tabs .spacer {
    flex: 1;
    align-self: stretch;
  }
  .tabs .right {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-bottom: 6px;
  }
  .rbtn-top {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 7px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .rbtn-top:hover:not(:disabled) {
    background: rgba(127, 127, 127, 0.12);
    color: var(--text);
  }
  .rbtn-top:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* command toolbar */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 7px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .tbtn {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 7px;
    color: var(--text);
    cursor: pointer;
  }
  .tbtn:hover {
    background: var(--canvas);
  }
  .tbtn.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 4px;
  }
  /* input angka (halaman & zoom) */
  .num-in {
    width: 44px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--canvas);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    text-align: center;
    font-variant-numeric: tabular-nums;
    outline: none;
  }
  .num-in:focus {
    border-color: var(--accent);
  }
  .of {
    font-size: 13px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .zoom-slider {
    width: 96px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .zoom-field {
    position: relative;
    display: flex;
    align-items: center;
  }
  .zoom-in {
    width: 56px;
    border-radius: 6px 0 0 6px;
    border-right: none;
  }
  .caret {
    height: 26px;
    width: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 0 6px 6px 0;
    background: var(--canvas);
    color: var(--text-muted);
    cursor: pointer;
  }
  .caret:hover {
    color: var(--text);
  }
  .zoom-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    min-width: 110px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow-float);
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .zoom-menu button {
    text-align: left;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    border-radius: 5px;
    cursor: pointer;
  }
  .zoom-menu button:hover {
    background: var(--canvas);
  }
  .zm-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 2px;
  }
  .grow {
    flex: 1;
  }

  /* body */
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    background: var(--sunken);
  }

  /* left rail */
  .rail {
    width: 48px;
    flex: none;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 0;
  }
  .rbtn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 8px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .rbtn:hover {
    background: var(--canvas);
    color: var(--text);
  }
  .rbtn.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  /* placeholder panel */
  .panel {
    width: 220px;
    flex: none;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
  }
  .panel-head {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 14px 16px 8px;
  }

  /* viewport */
  .view-wrap {
    flex: 1;
    min-width: 0;
    position: relative;
    display: flex;
  }

  /* find bar */
  .findbar {
    position: absolute;
    top: 12px;
    right: 16px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow-float);
    color: var(--text-muted);
  }
  .findbar input {
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    width: 180px;
    outline: none;
    padding: 2px 4px;
  }
  .findbar .count {
    font-size: 12px;
    color: var(--text-muted);
    min-width: 40px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .fb {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 6px;
    color: var(--text);
    cursor: pointer;
  }
  .fb:hover:not(:disabled) {
    background: var(--canvas);
  }
  .fb:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* floating pill */
  .pill {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--surface);
    box-shadow: var(--shadow-float);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 5px 8px;
    z-index: 5;
  }
  .pb {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    border-radius: 50%;
    color: var(--text);
    cursor: pointer;
  }
  .pb:hover {
    background: var(--canvas);
  }
  .pill .lbl {
    font-size: 12px;
    padding: 0 4px;
    font-variant-numeric: tabular-nums;
  }
  .pill .vsep {
    width: 1px;
    height: 16px;
    background: var(--border);
    margin: 0 2px;
  }

  /* states */
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .muted {
    color: var(--text-muted);
  }
  .err {
    color: var(--destructive);
    max-width: 520px;
    word-break: break-word;
  }
  .empty-logo {
    opacity: 0.92;
  }
  .empty h1 {
    margin: 14px 0 4px;
    font-size: 22px;
  }
  .open-cta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    height: 38px;
    padding: 0 18px;
    border: none;
    border-radius: var(--r-ctrl);
    background: var(--accent);
    color: #fff;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .open-cta:hover {
    background: var(--accent-hover);
  }
</style>
