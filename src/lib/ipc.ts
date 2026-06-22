import { invoke } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";

export type PageDim = { width: number; height: number }; // dalam points (1/72")
export type DocInfo = { id: number; page_count: number; pages: PageDim[] };
export type OutlineItem = {
  title: string;
  page: number | null; // indeks halaman 0-based, atau null
  children: OutlineItem[];
};
export type MatchRect = { x: number; y: number; w: number; h: number }; // points, origin atas-kiri
export type SearchMatch = { page: number; rects: MatchRect[] };

/** Dialog pilih satu/banyak PDF sekaligus. Kembalikan daftar path (bisa kosong). */
export async function pickPdfs(): Promise<string[]> {
  const res = await openFileDialog({
    multiple: true,
    directory: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (Array.isArray(res)) return res;
  return typeof res === "string" ? [res] : [];
}

/** Buka dokumen di backend; balas info halaman. */
export async function openDocument(path: string): Promise<DocInfo> {
  return await invoke<DocInfo>("open_document", { path });
}

/** Render satu halaman dari dokumen `docId` ke PNG (skala = piksel per point).
 *  Dipakai untuk thumbnail (kecil; PNG murah). */
export async function renderPage(
  docId: number,
  index: number,
  scale: number,
): Promise<Blob> {
  const bytes = await invoke<ArrayBuffer>("render_page", { docId, index, scale });
  return new Blob([bytes], { type: "image/png" });
}

/** Render halaman ke RGBA mentah → ImageBitmap (untuk viewport utama, via
 *  <canvas>). Tanpa encode/decode PNG → jauh lebih cepat. Format buffer:
 *  [width u32 LE][height u32 LE][rgba...]. */
export async function renderPageRaw(
  docId: number,
  index: number,
  scale: number,
): Promise<ImageBitmap> {
  const buf = await invoke<ArrayBuffer>("render_page_raw", { docId, index, scale });
  const view = new DataView(buf);
  const w = view.getUint32(0, true);
  const h = view.getUint32(4, true);
  const data = new Uint8ClampedArray(buf, 8, w * h * 4);
  return await createImageBitmap(new ImageData(data, w, h));
}

/** Tutup dokumen di backend & bebaskan memorinya (saat tab ditutup). */
export async function closeDocument(docId: number): Promise<void> {
  await invoke("close_document", { docId });
}

/** Ambil outline/daftar isi (bookmark) dokumen. Kosong jika PDF tak punya. */
export async function getOutline(docId: number): Promise<OutlineItem[]> {
  return await invoke<OutlineItem[]>("get_outline", { docId });
}

/** Cari teks di dokumen; balikkan daftar kecocokan + persegi highlight. */
export async function searchDocument(
  docId: number,
  query: string,
): Promise<SearchMatch[]> {
  return await invoke<SearchMatch[]>("search_document", { docId, query });
}

export type PrinterList = { printers: string[]; default: string | null };
export type PaperSize = { id: number; name: string };
export type PrintOpts = {
  printer: string;
  copies: number;
  pages: number[]; // indeks 0-based
  grayscale: boolean;
  scale_mode: "fit" | "actual" | "custom";
  custom_scale: number; // persen
  paper: number; // id kertas DEVMODE (0 = default printer)
  paper_w: number; // lebar custom (0.1 mm; 0 = tidak)
  paper_h: number; // tinggi custom (0.1 mm)
  orientation: number; // 0=default, 1=portrait, 2=landscape
};

/** Daftar printer + default untuk dialog cetak. */
export async function listPrinters(): Promise<PrinterList> {
  return await invoke<PrinterList>("list_printers");
}

/** Daftar ukuran kertas yang didukung printer. */
export async function listPapers(printer: string): Promise<PaperSize[]> {
  return await invoke<PaperSize[]>("list_papers", { printer });
}

/** Cetak dokumen ke printer (raster). */
export async function printDocument(
  docId: number,
  opts: PrintOpts,
): Promise<void> {
  await invoke("print_document", { docId, opts });
}

/** Nama file dari sebuah path (Windows / POSIX). */
export function basename(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}
