//! Engine PDF berbasis PDFium (pdfium.dll).
//!
//! PDFium single-threaded & handle-nya `!Send`. Kita pakai POOL beberapa worker
//! thread (pola "actor"), tiap worker punya instance Pdfium + salinan dokumen
//! sendiri sehingga render banyak halaman bisa jalan PARALEL. Doc id dialokasikan
//! terpusat lalu Open di-broadcast ke semua worker agar id konsisten; Render
//! didistribusi round-robin. Handle PDFium tak pernah menyeberang thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

/// Ukuran satu halaman dalam "points" PDF (1 pt = 1/72 inci).
#[derive(Serialize, Clone)]
pub struct PageDim {
    pub width: f32,
    pub height: f32,
}

/// Info dokumen yang dikirim ke frontend saat membuka file.
/// `id` mengidentifikasi dokumen di backend (dipakai untuk render/close per-tab).
#[derive(Serialize, Clone)]
pub struct DocInfo {
    pub id: u32,
    pub page_count: usize,
    pub pages: Vec<PageDim>,
}

/// Satu entri outline/bookmark (daftar isi) PDF, bisa bersarang.
#[derive(Serialize, Clone)]
pub struct OutlineItem {
    pub title: String,
    pub page: Option<usize>, // indeks halaman 0-based, jika ada tujuan
    pub children: Vec<OutlineItem>,
}

/// Persegi highlight (dalam points, origin atas-kiri agar gampang dipakai di CSS).
#[derive(Serialize, Clone)]
pub struct MatchRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Satu kecocokan hasil pencarian: halaman + persegi-persegi (per segmen teks).
#[derive(Serialize, Clone)]
pub struct SearchMatch {
    pub page: usize,
    pub rects: Vec<MatchRect>,
}

/// Opsi cetak dari frontend (dialog print).
#[derive(Deserialize, Clone)]
pub struct PrintOpts {
    pub printer: String,
    pub copies: u32,
    pub pages: Vec<usize>,   // indeks 0-based, sudah di-expand dari range
    pub grayscale: bool,
    pub scale_mode: String,  // "fit" | "actual" | "custom"
    pub custom_scale: f32,   // persen (untuk "custom")
    pub paper: u16,          // id ukuran kertas DEVMODE (0 = default printer)
    pub paper_w: u16,        // lebar kertas custom (0.1 mm; 0 = tidak)
    pub paper_h: u16,        // tinggi kertas custom (0.1 mm)
    pub orientation: u16,    // 0=default, 1=portrait, 2=landscape
}

enum Cmd {
    Open {
        id: u32, // id dialokasikan terpusat agar konsisten di semua worker
        path: String,
        resp: Sender<Result<DocInfo, String>>,
    },
    /// Render ke PNG (dipakai thumbnail; kecil, encode murah).
    Render {
        doc_id: u32,
        index: usize,
        scale: f32,
        resp: Sender<Result<Vec<u8>, String>>,
    },
    /// Render ke RGBA mentah (header w/h + pixel) — tanpa encode PNG, untuk
    /// viewport utama. Jauh lebih murah per halaman.
    RenderRaw {
        doc_id: u32,
        index: usize,
        scale: f32,
        resp: Sender<Result<Vec<u8>, String>>,
    },
    Close {
        doc_id: u32,
    },
    Outline {
        doc_id: u32,
        resp: Sender<Result<Vec<OutlineItem>, String>>,
    },
    Search {
        doc_id: u32,
        query: String,
        resp: Sender<Result<Vec<SearchMatch>, String>>,
    },
    Print {
        doc_id: u32,
        opts: PrintOpts,
        progress: Box<dyn Fn(usize, usize) + Send>,
        resp: Sender<Result<(), String>>,
    },
}

/// Pool worker PDFium. Disimpan sebagai Tauri managed state.
pub struct PdfEngine {
    workers: Mutex<Vec<Sender<Cmd>>>,
    next_id: Arc<AtomicU32>,
    rr: Arc<AtomicUsize>, // penghitung round-robin untuk distribusi render
}

impl PdfEngine {
    pub fn new() -> Result<Self, String> {
        // PDFium punya inisialisasi library GLOBAL (FPDF_InitLibrary) yang hanya
        // boleh sekali & TIDAK thread-safe untuk render paralel. Maka 1 worker
        // saja. (Struktur pool dipertahankan agar mudah diubah bila suatu saat
        // pakai PDFium thread-safe / multi-proses.) Percepatan nyata datang dari
        // pipeline RGBA mentah (tanpa encode PNG), bukan paralelisme.
        let n = 1;

        let mut workers = Vec::with_capacity(n);
        let mut readys = Vec::with_capacity(n);
        for _ in 0..n {
            let (tx, rx) = mpsc::channel::<Cmd>();
            let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
            thread::spawn(move || run_worker(rx, ready_tx));
            workers.push(tx);
            readys.push(ready_rx);
        }
        for r in readys {
            r.recv()
                .map_err(|e| format!("worker PDF tidak merespons: {e}"))??;
        }

        Ok(Self {
            workers: Mutex::new(workers),
            next_id: Arc::new(AtomicU32::new(1)),
            rr: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Ambil handle yang bisa dipindah ke blocking thread (lihat command async).
    pub fn handle(&self) -> Result<PdfHandle, String> {
        let workers = self
            .workers
            .lock()
            .map_err(|_| "engine terkunci".to_string())?
            .clone();
        Ok(PdfHandle {
            workers,
            next_id: self.next_id.clone(),
            rr: self.rr.clone(),
        })
    }
}

/// Loop satu worker: punya instance Pdfium + peta dokumen sendiri. Meminjam
/// `pdfium` selama loop, jadi lifetime aman tanpa unsafe.
fn run_worker(rx: mpsc::Receiver<Cmd>, ready_tx: Sender<Result<(), String>>) {
    let pdfium = match make_pdfium() {
        Ok(p) => p,
        Err(e) => {
            let _ = ready_tx.send(Err(e));
            return;
        }
    };
    let _ = ready_tx.send(Ok(()));

    let mut docs: HashMap<u32, PdfDocument> = HashMap::new();

    for cmd in rx {
        match cmd {
            Cmd::Open { id, path, resp } => {
                let result = match pdfium.load_pdf_from_file(&path, None) {
                    Ok(d) => {
                        let dims: Vec<PageDim> = d
                            .pages()
                            .iter()
                            .map(|p| PageDim {
                                width: p.width().value,
                                height: p.height().value,
                            })
                            .collect();
                        let count = dims.len();
                        docs.insert(id, d);
                        Ok(DocInfo {
                            id,
                            page_count: count,
                            pages: dims,
                        })
                    }
                    Err(e) => Err(format!("gagal membuka PDF: {e}")),
                };
                let _ = resp.send(result);
            }
            Cmd::Render {
                doc_id,
                index,
                scale,
                resp,
            } => {
                let _ = resp.send(render_page_to_png(docs.get(&doc_id), index, scale));
            }
            Cmd::RenderRaw {
                doc_id,
                index,
                scale,
                resp,
            } => {
                let _ = resp.send(render_page_raw(docs.get(&doc_id), index, scale));
            }
            Cmd::Close { doc_id } => {
                docs.remove(&doc_id);
            }
            Cmd::Outline { doc_id, resp } => {
                let result = match docs.get(&doc_id) {
                    Some(d) => Ok(build_outline(d)),
                    None => Err("belum ada dokumen yang terbuka".to_string()),
                };
                let _ = resp.send(result);
            }
            Cmd::Search {
                doc_id,
                query,
                resp,
            } => {
                let result = match docs.get(&doc_id) {
                    Some(d) => Ok(search_text(d, &query)),
                    None => Err("belum ada dokumen yang terbuka".to_string()),
                };
                let _ = resp.send(result);
            }
            Cmd::Print {
                doc_id,
                opts,
                progress,
                resp,
            } => {
                let result = match docs.get(&doc_id) {
                    Some(d) => print_doc(d, &opts, &*progress),
                    None => Err("belum ada dokumen yang terbuka".to_string()),
                };
                let _ = resp.send(result);
            }
        }
    }
}

/// Handle ringan & `Send` ke pool worker PDF. Dipakai di dalam `spawn_blocking`
/// agar operasi PDFium yang memblokir tak pernah menyentuh main/UI thread.
#[derive(Clone)]
pub struct PdfHandle {
    workers: Vec<Sender<Cmd>>,
    next_id: Arc<AtomicU32>,
    rr: Arc<AtomicUsize>,
}

impl PdfHandle {
    /// Pilih worker berikutnya (round-robin) untuk operasi render.
    fn pick(&self) -> &Sender<Cmd> {
        let i = self.rr.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        &self.workers[i]
    }

    pub fn open(&self, path: String) -> Result<DocInfo, String> {
        // Id terpusat -> broadcast ke SEMUA worker (tiap worker simpan salinan).
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut rxs = Vec::with_capacity(self.workers.len());
        for tx in &self.workers {
            let (resp, rx) = mpsc::channel();
            tx.send(Cmd::Open {
                id,
                path: path.clone(),
                resp,
            })
            .map_err(|e| e.to_string())?;
            rxs.push(rx);
        }
        // Tunggu semua selesai memuat; kembalikan hasil pertama (semua identik).
        let mut out: Option<Result<DocInfo, String>> = None;
        for rx in rxs {
            let r = rx.recv().map_err(|e| e.to_string())?;
            if out.is_none() {
                out = Some(r);
            }
        }
        out.unwrap_or_else(|| Err("tak ada worker PDF".to_string()))
    }

    pub fn render(&self, doc_id: u32, index: usize, scale: f32) -> Result<Vec<u8>, String> {
        let (resp, rx) = mpsc::channel();
        self.pick()
            .send(Cmd::Render {
                doc_id,
                index,
                scale,
                resp,
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn render_raw(&self, doc_id: u32, index: usize, scale: f32) -> Result<Vec<u8>, String> {
        let (resp, rx) = mpsc::channel();
        self.pick()
            .send(Cmd::RenderRaw {
                doc_id,
                index,
                scale,
                resp,
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn close(&self, doc_id: u32) -> Result<(), String> {
        for tx in &self.workers {
            let _ = tx.send(Cmd::Close { doc_id });
        }
        Ok(())
    }

    pub fn outline(&self, doc_id: u32) -> Result<Vec<OutlineItem>, String> {
        let (resp, rx) = mpsc::channel();
        self.workers[0]
            .send(Cmd::Outline { doc_id, resp })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn search(&self, doc_id: u32, query: String) -> Result<Vec<SearchMatch>, String> {
        let (resp, rx) = mpsc::channel();
        self.workers[0]
            .send(Cmd::Search {
                doc_id,
                query,
                resp,
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn print(
        &self,
        doc_id: u32,
        opts: PrintOpts,
        progress: Box<dyn Fn(usize, usize) + Send>,
    ) -> Result<(), String> {
        let (resp, rx) = mpsc::channel();
        self.workers[0]
            .send(Cmd::Print {
                doc_id,
                opts,
                progress,
                resp,
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
}

/// Cari `query` di semua halaman. Balikkan kecocokan + persegi highlight
/// (points, origin atas-kiri). Dibatasi agar query umum tak menggantung worker.
fn search_text(doc: &PdfDocument, query: &str) -> Vec<SearchMatch> {
    const MAX_MATCHES: usize = 5000;
    let mut out: Vec<SearchMatch> = Vec::new();
    if query.trim().is_empty() {
        return out;
    }
    let options = PdfSearchOptions::new();

    for (idx, page) in doc.pages().iter().enumerate() {
        let height = page.height().value;
        let text = match page.text() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let search = match text.search(query, &options) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for segments in search.iter(PdfSearchDirection::SearchForward) {
            let rects: Vec<MatchRect> = segments
                .iter()
                .map(|seg| {
                    let b = seg.bounds();
                    let left = b.left().value;
                    let top = b.top().value;
                    MatchRect {
                        x: left,
                        y: height - top, // origin bawah-kiri (PDF) -> atas-kiri
                        w: b.right().value - left,
                        h: top - b.bottom().value,
                    }
                })
                .collect();
            out.push(SearchMatch { page: idx, rects });
            if out.len() >= MAX_MATCHES {
                return out;
            }
        }
    }
    out
}

/// Bangun pohon outline/bookmark dokumen. Telusuri root + sibling di tiap level,
/// rekursif ke anak. Halaman tujuan diambil dari destination (0-based).
///
/// PENTING: pakai `budget` (jumlah node maksimum) + batas kedalaman agar PDF
/// dengan bookmark malformed/siklik tak membuat penelusuran loop tak berujung —
/// itu akan menggantung worker thread tunggal & menghentikan SEMUA render.
fn build_outline(doc: &PdfDocument) -> Vec<OutlineItem> {
    const MAX_NODES: usize = 50_000;
    const MAX_DEPTH: u32 = 32;

    let mut budget = MAX_NODES;
    let mut top = Vec::new();
    let mut node = doc.bookmarks().root();
    while let Some(bm) = node {
        if budget == 0 {
            break;
        }
        top.push(build_outline_node(&bm, 0, MAX_DEPTH, &mut budget));
        node = bm.next_sibling();
    }
    eprintln!(
        "[outline] total node terkunjungi: {} (cap {}){}",
        MAX_NODES - budget,
        MAX_NODES,
        if budget == 0 { " — KENA CAP (mungkin siklik!)" } else { "" }
    );
    top
}

fn build_outline_node(
    bm: &PdfBookmark,
    depth: u32,
    max_depth: u32,
    budget: &mut usize,
) -> OutlineItem {
    *budget = budget.saturating_sub(1);

    let title = bm.title().unwrap_or_default();
    let page = bookmark_page(bm);

    let mut children = Vec::new();
    if depth < max_depth {
        let mut child = bm.first_child();
        while let Some(c) = child {
            if *budget == 0 {
                break;
            }
            children.push(build_outline_node(&c, depth + 1, max_depth, budget));
            child = c.next_sibling();
        }
    }

    OutlineItem {
        title,
        page,
        children,
    }
}

/// Resolusi halaman tujuan sebuah bookmark: coba destinasi langsung, lalu
/// fallback ke action "GoTo dalam dokumen" (banyak PDF pakai action, bukan
/// dest langsung, sehingga `destination()` saja mengembalikan None).
fn bookmark_page(bm: &PdfBookmark) -> Option<usize> {
    if let Some(d) = bm.destination() {
        if let Ok(i) = d.page_index() {
            return Some(i as usize);
        }
    }
    if let Some(PdfAction::LocalDestination(local)) = bm.action() {
        if let Ok(d) = local.destination() {
            if let Ok(i) = d.page_index() {
                return Some(i as usize);
            }
        }
    }
    None
}

/// Render satu halaman ke PNG pada skala tertentu (piksel per point).
fn render_page_to_png(
    doc: Option<&PdfDocument>,
    index: usize,
    scale: f32,
) -> Result<Vec<u8>, String> {
    let doc = doc.ok_or("belum ada dokumen yang terbuka")?;
    let page = doc
        .pages()
        .get((index as u16).into())
        .map_err(|e| format!("halaman {index} tidak ada: {e}"))?;

    let w_px = (page.width().value * scale).round().max(1.0) as i32;
    let h_px = (page.height().value * scale).round().max(1.0) as i32;

    let config = PdfRenderConfig::new()
        .set_target_width(w_px)
        .set_target_height(h_px);

    let image = page
        .render_with_config(&config)
        .map_err(|e| format!("gagal render: {e}"))?
        .as_image()
        .map_err(|e| format!("gagal konversi gambar: {e}"))?;

    // PNG mode cepat: kompresi minimal + tanpa filter. Untuk halaman teks,
    // ukuran tetap kecil tapi waktu encode jauh lebih singkat (lebih "sat set").
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;

    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::NoFilter)
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            image.color().into(),
        )
        .map_err(|e| format!("gagal encode PNG: {e}"))?;
    Ok(png)
}

/// Render satu halaman ke RGBA MENTAH (tanpa encode PNG). Format keluaran:
/// `[width: u32 LE][height: u32 LE][rgba bytes...]`. Frontend membuat
/// `ImageBitmap` langsung dari ini → gambar ke `<canvas>`. Menghindari biaya
/// encode/decode PNG → jauh lebih cepat per halaman.
fn render_page_raw(
    doc: Option<&PdfDocument>,
    index: usize,
    scale: f32,
) -> Result<Vec<u8>, String> {
    let doc = doc.ok_or("belum ada dokumen yang terbuka")?;
    let (w, h, raw) = render_rgba(doc, index, scale)?;
    let mut out = Vec::with_capacity(8 + raw.len());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&raw);
    Ok(out)
}

/// Render halaman ke RGBA mentah → (width, height, bytes).
fn render_rgba(doc: &PdfDocument, index: usize, scale: f32) -> Result<(u32, u32, Vec<u8>), String> {
    let page = doc
        .pages()
        .get((index as u16).into())
        .map_err(|e| format!("halaman {index} tidak ada: {e}"))?;

    let w_px = (page.width().value * scale).round().max(1.0) as i32;
    let h_px = (page.height().value * scale).round().max(1.0) as i32;

    let config = PdfRenderConfig::new()
        .set_target_width(w_px)
        .set_target_height(h_px);

    let rgba = page
        .render_with_config(&config)
        .map_err(|e| format!("gagal render: {e}"))?
        .as_image()
        .map_err(|e| format!("gagal konversi gambar: {e}"))?
        .into_rgba8();

    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Konversi RGBA → BGRA (urutan yang diharapkan DIB GDI). Jika `grayscale`,
/// ganti tiap piksel dengan luminansinya.
fn to_bgra(buf: &mut [u8], grayscale: bool) {
    for px in buf.chunks_exact_mut(4) {
        let (r, g, b) = (px[0], px[1], px[2]);
        if grayscale {
            let y = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as u8;
            px[0] = y;
            px[1] = y;
            px[2] = y;
        } else {
            px[0] = b;
            px[2] = r;
        }
    }
}

/// Cetak halaman-halaman terpilih ke printer (raster). Dijalankan di worker
/// (akses PDFium). Render tiap halaman pada DPI tetap lalu di-stretch ke ukuran
/// target sesuai mode skala, dengan auto-center.
fn print_doc(
    doc: &PdfDocument,
    opts: &PrintOpts,
    progress: &dyn Fn(usize, usize),
) -> Result<(), String> {
    const RENDER_DPI: f32 = 200.0;

    if opts.pages.is_empty() {
        return Err("tak ada halaman untuk dicetak".to_string());
    }

    let spec = crate::print::DevmodeSpec {
        paper: opts.paper,
        paper_w: opts.paper_w,
        paper_h: opts.paper_h,
        orientation: opts.orientation,
    };
    let job = crate::print::PrintJob::start(&opts.printer, "Feather PDF", &spec)?;
    let (dpi_x, dpi_y, printable_w, printable_h) = job.caps();
    let total = opts.pages.len() * opts.copies.max(1) as usize;
    let mut done = 0usize;

    let result = (|| -> Result<(), String> {
        for _ in 0..opts.copies.max(1) {
            for &idx in &opts.pages {
                let page = doc
                    .pages()
                    .get((idx as u16).into())
                    .map_err(|e| format!("halaman {idx} tidak ada: {e}"))?;
                let pin_w = page.width().value / 72.0; // inci
                let pin_h = page.height().value / 72.0;

                // Render bitmap sumber.
                let (w, h, mut rgba) = render_rgba(doc, idx, RENDER_DPI / 72.0)?;
                to_bgra(&mut rgba, opts.grayscale);

                // Ukuran target (device px) sesuai mode skala.
                let actual_w = pin_w * dpi_x as f32;
                let actual_h = pin_h * dpi_y as f32;
                let (tw, th) = match opts.scale_mode.as_str() {
                    "actual" => (actual_w, actual_h),
                    "custom" => {
                        let s = opts.custom_scale / 100.0;
                        (actual_w * s, actual_h * s)
                    }
                    _ => {
                        // fit: muat dalam area tercetak, jaga rasio
                        let s = (printable_w as f32 / actual_w)
                            .min(printable_h as f32 / actual_h);
                        (actual_w * s, actual_h * s)
                    }
                };
                let tw = (tw.round() as i32).max(1);
                let th = (th.round() as i32).max(1);
                let dx = (printable_w - tw) / 2; // auto-center
                let dy = (printable_h - th) / 2;

                job.page(&rgba, w as i32, h as i32, dx, dy, tw, th)?;
                done += 1;
                progress(done, total);
            }
        }
        Ok(())
    })();

    // Selalu tutup dokumen, apa pun hasilnya.
    job.finish()?;
    result
}

/// Cari & ikat pdfium.dll: coba direktori exe dulu, lalu cwd.
fn make_pdfium() -> Result<Pdfium, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    dirs.push(PathBuf::from("."));

    let mut last_err = String::from("tak ada kandidat path");
    for dir in dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        match Pdfium::bind_to_library(&lib) {
            Ok(bindings) => return Ok(Pdfium::new(bindings)),
            Err(e) => last_err = format!("{e} @ {}", lib.display()),
        }
    }
    Err(format!("pdfium.dll tidak bisa dimuat: {last_err}"))
}
