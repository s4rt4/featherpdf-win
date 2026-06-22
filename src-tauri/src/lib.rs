mod pdf;
mod print;

use pdf::{DocInfo, OutlineItem, PdfEngine, PrintOpts, SearchMatch};
use serde::Serialize;
use tauri::{Emitter, State};

/// Daftar printer yang tersedia + default.
#[derive(Serialize)]
struct PrinterList {
    printers: Vec<String>,
    default: Option<String>,
}

/// Versi backend — dipakai sebagai tes IPC end-to-end.
#[tauri::command]
fn app_version() -> String {
    format!("feather-pdf v{}", env!("CARGO_PKG_VERSION"))
}

/// Buka PDF dari path, kembalikan jumlah & ukuran tiap halaman.
/// `async` agar tak memblokir main/UI thread; kerja blocking di thread pool.
#[tauri::command]
async fn open_document(
    state: State<'_, PdfEngine>,
    path: String,
) -> Result<DocInfo, String> {
    let handle = state.handle()?;
    tauri::async_runtime::spawn_blocking(move || handle.open(path))
        .await
        .map_err(|e| e.to_string())?
}

/// Render satu halaman dari dokumen `doc_id` ke PNG (skala = piksel per point),
/// balas sebagai biner.
#[tauri::command]
async fn render_page(
    state: State<'_, PdfEngine>,
    doc_id: u32,
    index: usize,
    scale: f32,
) -> Result<tauri::ipc::Response, String> {
    let handle = state.handle()?;
    let png = tauri::async_runtime::spawn_blocking(move || handle.render(doc_id, index, scale))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(png))
}

/// Render satu halaman ke RGBA mentah (header w/h + pixel) — viewport utama.
#[tauri::command]
async fn render_page_raw(
    state: State<'_, PdfEngine>,
    doc_id: u32,
    index: usize,
    scale: f32,
) -> Result<tauri::ipc::Response, String> {
    let handle = state.handle()?;
    let bytes = tauri::async_runtime::spawn_blocking(move || handle.render_raw(doc_id, index, scale))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Tutup dokumen `doc_id` dan bebaskan memorinya (dipanggil saat tab ditutup).
#[tauri::command]
async fn close_document(state: State<'_, PdfEngine>, doc_id: u32) -> Result<(), String> {
    let handle = state.handle()?;
    tauri::async_runtime::spawn_blocking(move || handle.close(doc_id))
        .await
        .map_err(|e| e.to_string())?
}

/// Ambil outline/daftar isi (bookmark) dokumen `doc_id`.
#[tauri::command]
async fn get_outline(
    state: State<'_, PdfEngine>,
    doc_id: u32,
) -> Result<Vec<OutlineItem>, String> {
    let handle = state.handle()?;
    tauri::async_runtime::spawn_blocking(move || handle.outline(doc_id))
        .await
        .map_err(|e| e.to_string())?
}

/// Cari teks `query` di dokumen `doc_id`; balikkan kecocokan + persegi highlight.
#[tauri::command]
async fn search_document(
    state: State<'_, PdfEngine>,
    doc_id: u32,
    query: String,
) -> Result<Vec<SearchMatch>, String> {
    let handle = state.handle()?;
    tauri::async_runtime::spawn_blocking(move || handle.search(doc_id, query))
        .await
        .map_err(|e| e.to_string())?
}

/// Daftar printer untuk dialog cetak.
#[tauri::command]
async fn list_printers() -> Result<PrinterList, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let (printers, default) = print::list_printers();
        PrinterList { printers, default }
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct PaperSize {
    id: u16,
    name: String,
}

/// Daftar ukuran kertas yang didukung printer tertentu.
#[tauri::command]
async fn list_papers(printer: String) -> Result<Vec<PaperSize>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        print::list_papers(&printer)
            .into_iter()
            .map(|(id, name)| PaperSize { id, name })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Cetak dokumen `doc_id` ke printer (raster) sesuai opsi. Memancarkan event
/// `print-progress` (done, total) selama mencetak.
#[tauri::command]
async fn print_document(
    app: tauri::AppHandle,
    state: State<'_, PdfEngine>,
    doc_id: u32,
    opts: PrintOpts,
) -> Result<(), String> {
    let handle = state.handle()?;
    let progress = Box::new(move |done: usize, total: usize| {
        let _ = app.emit("print-progress", (done, total));
    });
    tauri::async_runtime::spawn_blocking(move || handle.print(doc_id, opts, progress))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = PdfEngine::new().expect("gagal inisialisasi engine PDFium");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            app_version,
            open_document,
            render_page,
            render_page_raw,
            close_document,
            get_outline,
            search_document,
            list_printers,
            list_papers,
            print_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
