<script lang="ts">
  import { X } from "@lucide/svelte";

  let { onClose }: { onClose: () => void } = $props();

  // Kelompok pintasan. Tiap tombol jadi <kbd>; gabung dgn "+".
  const GROUPS: { title: string; items: { keys: string[]; desc: string }[] }[] = [
    {
      title: "File & tab",
      items: [
        { keys: ["Ctrl", "O"], desc: "Buka PDF" },
        { keys: ["Ctrl", "W"], desc: "Tutup tab" },
        { keys: ["Ctrl", "P"], desc: "Cetak" },
      ],
    },
    {
      title: "Navigasi",
      items: [
        { keys: ["Ctrl", "F"], desc: "Cari di dokumen" },
        { keys: ["Enter"], desc: "Hasil cari berikutnya" },
        { keys: ["Shift", "Enter"], desc: "Hasil cari sebelumnya" },
      ],
    },
    {
      title: "Tampilan",
      items: [
        { keys: ["Ctrl", "+"], desc: "Perbesar" },
        { keys: ["Ctrl", "−"], desc: "Perkecil" },
        { keys: ["Ctrl", "Scroll"], desc: "Zoom dgn roda mouse" },
        { keys: ["F11"], desc: "Layar penuh" },
        { keys: ["Esc"], desc: "Tutup dialog / keluar layar penuh" },
        { keys: ["?"], desc: "Tampilkan pintasan ini" },
      ],
    },
  ];
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="dialog" role="dialog" aria-label="Pintasan keyboard">
    <div class="head">
      <span class="title">Pintasan keyboard</span>
      <button class="x" title="Tutup (Esc)" onclick={onClose}><X size={18} /></button>
    </div>
    <div class="body">
      {#each GROUPS as g}
        <div class="group">
          <div class="g-title">{g.title}</div>
          {#each g.items as it}
            <div class="srow">
              <span class="keys">
                {#each it.keys as k, i}
                  {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k}</kbd>
                {/each}
              </span>
              <span class="desc">{it.desc}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 320;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dialog {
    width: 560px;
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
  .body {
    padding: 8px 16px 16px;
    overflow: auto;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 28px;
  }
  .group {
    padding-top: 10px;
  }
  .g-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    margin-bottom: 8px;
  }
  .srow {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 0;
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: none;
    min-width: 116px;
  }
  .plus {
    color: var(--text-muted);
    font-size: 11px;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    color: var(--text);
    background: var(--canvas);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
  }
  .desc {
    font-size: 13px;
    color: var(--text);
  }
</style>
