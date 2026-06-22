// Store tab multi-dokumen. Tiap tab memegang info dokumen + state tampilan
// (zoom, fit, halaman aktif) sendiri-sendiri, ala Acrobat.
import { openDocument, closeDocument, basename, type DocInfo } from "./ipc";

export type FitMode = "width" | "page" | "custom";
export type Layout = "single" | "continuous" | "facing" | "facing-cont";

export type Tab = {
  id: number; // id tab di frontend (bukan id dokumen backend)
  doc: DocInfo; // doc.id = id dokumen di backend
  path: string;
  name: string;
  zoom: number;
  fitMode: FitMode;
  layout: Layout;
  page: number; // halaman aktif (1-based) untuk counter
  scrollTop: number; // posisi scroll terakhir (dipulihkan saat balik ke tab)
};

class TabStore {
  tabs = $state<Tab[]>([]);
  activeId = $state<number | null>(null);
  #nextId = 1;

  get active(): Tab | null {
    return this.tabs.find((t) => t.id === this.activeId) ?? null;
  }

  /**
   * Buka file di tab baru (atau aktifkan kalau sudah terbuka).
   * `activate=false` dipakai saat buka banyak file sekaligus: cuma file
   * pertama yang difokuskan, sisanya dimuat di latar tanpa merebut fokus.
   */
  async open(path: string, activate = true): Promise<Tab> {
    const existing = this.tabs.find((t) => t.path === path);
    if (existing) {
      if (activate) this.activeId = existing.id;
      return existing;
    }
    const doc = await openDocument(path);
    const tab: Tab = {
      id: this.#nextId++,
      doc,
      path,
      name: basename(path),
      zoom: 1,
      fitMode: "width",
      layout: "continuous",
      page: 1,
      scrollTop: 0,
    };
    this.tabs.push(tab);
    if (activate || this.activeId === null) this.activeId = tab.id;
    return tab;
  }

  /** Tutup tab & bebaskan dokumennya di backend. Pilih tab pengganti. */
  close(id: number): void {
    const idx = this.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return;
    const [tab] = this.tabs.splice(idx, 1);
    void closeDocument(tab.doc.id).catch(() => {});
    if (this.activeId === id) {
      const next = this.tabs[idx] ?? this.tabs[idx - 1] ?? null;
      this.activeId = next ? next.id : null;
    }
  }

  activate(id: number): void {
    this.activeId = id;
  }
}

export const tabStore = new TabStore();
