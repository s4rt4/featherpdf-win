//! Cetak ke printer Windows lewat GDI (print-as-image / raster).
//!
//! Render halaman PDF → RGBA (di worker PDFium) → konversi ke BGRA top-down →
//! `StretchDIBits` ke device context printer. Mendukung skala fit/aktual/custom,
//! grayscale, dan auto-center. Hanya dikompilasi di Windows.

use std::mem::size_of;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, GetDeviceCaps, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DEVMODEW, DEVMODE_FIELD_FLAGS, DIB_RGB_COLORS, DM_IN_BUFFER,
    DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERLENGTH, DM_PAPERSIZE, DM_PAPERWIDTH, HALFTONE, HDC,
    HORZRES, LOGPIXELSX, LOGPIXELSY, SRCCOPY, VERTRES,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
    PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
};
// Fungsi dokumen cetak GDI & DeviceCapabilities ada di modul Xps pada windows-rs.
use windows::Win32::Storage::Xps::{
    DeviceCapabilitiesW, EndDoc, EndPage, StartDocW, StartPage, DC_PAPERNAMES, DC_PAPERS, DOCINFOW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Daftar nama printer + printer default (jika ada).
pub fn list_printers() -> (Vec<String>, Option<String>) {
    let mut names: Vec<String> = Vec::new();
    let mut default: Option<String> = None;

    unsafe {
        // Printer default: panggilan pertama untuk dapat panjang buffer.
        let mut len: u32 = 0;
        let _ = GetDefaultPrinterW(PWSTR::null(), &mut len);
        if len > 1 {
            let mut buf = vec![0u16; len as usize];
            if GetDefaultPrinterW(PWSTR(buf.as_mut_ptr()), &mut len).as_bool() {
                default = Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]));
            }
        }

        // Enumerasi (level 4 = cepat, hanya nama).
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;
        // Panggilan pertama untuk ukuran buffer (akan Err krn buffer kosong).
        let _ = EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut returned);
        if needed > 0 {
            let mut buf = vec![0u8; needed as usize];
            if EnumPrintersW(
                flags,
                PCWSTR::null(),
                4,
                Some(&mut buf),
                &mut needed,
                &mut returned,
            )
            .is_ok()
            {
                let infos = buf.as_ptr() as *const PRINTER_INFO_4W;
                for i in 0..returned as usize {
                    let info = &*infos.add(i);
                    if !info.pPrinterName.is_null() {
                        if let Ok(name) = info.pPrinterName.to_string() {
                            names.push(name);
                        }
                    }
                }
            }
        }
    }

    (names, default)
}

/// Daftar ukuran kertas yang didukung printer: (id DEVMODE, nama).
pub fn list_papers(printer: &str) -> Vec<(u16, String)> {
    let dev = wide(printer);
    let mut out: Vec<(u16, String)> = Vec::new();
    unsafe {
        let n = DeviceCapabilitiesW(
            PCWSTR(dev.as_ptr()),
            PCWSTR::null(),
            DC_PAPERS,
            PWSTR::null(),
            None,
        );
        if n <= 0 {
            return out;
        }
        let n = n as usize;

        // ID kertas (tiap entri 1 WORD).
        let mut ids = vec![0u16; n];
        DeviceCapabilitiesW(
            PCWSTR(dev.as_ptr()),
            PCWSTR::null(),
            DC_PAPERS,
            PWSTR(ids.as_mut_ptr()),
            None,
        );

        // Nama kertas (tiap entri 64 wchar).
        let mut names = vec![0u16; n * 64];
        let nn = DeviceCapabilitiesW(
            PCWSTR(dev.as_ptr()),
            PCWSTR::null(),
            DC_PAPERNAMES,
            PWSTR(names.as_mut_ptr()),
            None,
        );
        let count = if nn > 0 { n.min(nn as usize) } else { 0 };
        for i in 0..count {
            let slice = &names[i * 64..i * 64 + 64];
            let end = slice.iter().position(|&c| c == 0).unwrap_or(64);
            out.push((ids[i], String::from_utf16_lossy(&slice[..end])));
        }
    }
    out
}

/// Spesifikasi kertas & orientasi untuk DEVMODE. Semua 0 = pakai default printer.
#[derive(Default)]
pub struct DevmodeSpec {
    pub paper: u16,       // id kertas DEVMODE (0 = abaikan)
    pub paper_w: u16,     // lebar custom (0.1 mm; >0 = pakai ukuran custom)
    pub paper_h: u16,     // tinggi custom (0.1 mm)
    pub orientation: u16, // 0=default, 1=portrait, 2=landscape
}

impl DevmodeSpec {
    fn is_default(&self) -> bool {
        self.paper == 0 && self.paper_w == 0 && self.orientation == 0
    }
}

/// Bangun DEVMODE default printer lalu terapkan kertas/orientasi sesuai `spec`.
/// Mengembalikan buffer mentah (DEVMODE + driver extra) yang harus hidup selama
/// dipakai CreateDC. None bila tak ada yang perlu diubah / gagal.
fn build_devmode(printer: &str, spec: &DevmodeSpec) -> Option<Vec<u8>> {
    if spec.is_default() {
        return None;
    }
    let dev = wide(printer);
    unsafe {
        let mut hprinter = HANDLE::default();
        if OpenPrinterW(PCWSTR(dev.as_ptr()), &mut hprinter, None).is_err() {
            return None;
        }
        let needed = DocumentPropertiesW(
            HWND::default(),
            hprinter,
            PCWSTR(dev.as_ptr()),
            None,
            None,
            0,
        );
        if needed <= 0 {
            let _ = ClosePrinter(hprinter);
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        let dm = buf.as_mut_ptr() as *mut DEVMODEW;
        if DocumentPropertiesW(
            HWND::default(),
            hprinter,
            PCWSTR(dev.as_ptr()),
            Some(dm),
            None,
            DM_OUT_BUFFER.0,
        ) < 0
        {
            let _ = ClosePrinter(hprinter);
            return None;
        }

        // Field di union anonim DEVMODEW (Anonymous1.Anonymous1).
        let a = &mut (*dm).Anonymous1.Anonymous1;
        let mut fields = (*dm).dmFields.0;
        if spec.paper_w > 0 && spec.paper_h > 0 {
            a.dmPaperSize = 256; // DMPAPER_USER
            a.dmPaperWidth = spec.paper_w as i16;
            a.dmPaperLength = spec.paper_h as i16;
            fields |= DM_PAPERSIZE.0 | DM_PAPERWIDTH.0 | DM_PAPERLENGTH.0;
        } else if spec.paper != 0 {
            a.dmPaperSize = spec.paper as i16;
            fields |= DM_PAPERSIZE.0;
        }
        if spec.orientation != 0 {
            a.dmOrientation = spec.orientation as i16; // 1=portrait, 2=landscape
            fields |= DM_ORIENTATION.0;
        }
        (*dm).dmFields = DEVMODE_FIELD_FLAGS(fields);

        // Validasi/merge oleh driver.
        let _ = DocumentPropertiesW(
            HWND::default(),
            hprinter,
            PCWSTR(dev.as_ptr()),
            Some(dm),
            Some(dm),
            DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
        );
        let _ = ClosePrinter(hprinter);
        Some(buf)
    }
}

/// Pekerjaan cetak aktif (memegang device context printer).
pub struct PrintJob {
    hdc: HDC,
}

impl PrintJob {
    /// Buka printer & mulai dokumen, dengan kertas/orientasi sesuai `spec`.
    pub fn start(
        printer: &str,
        doc_name: &str,
        spec: &DevmodeSpec,
    ) -> Result<PrintJob, String> {
        let dev = wide(printer);
        // DEVMODE (kalau perlu). Buffer harus hidup selama pemanggilan CreateDCW.
        let dm_buf = build_devmode(printer, spec);
        let pdm = dm_buf.as_ref().map(|b| b.as_ptr() as *const DEVMODEW);
        let hdc =
            unsafe { CreateDCW(PCWSTR::null(), PCWSTR(dev.as_ptr()), PCWSTR::null(), pdm) };
        if hdc.is_invalid() {
            return Err(format!("tak bisa membuka printer '{printer}'"));
        }
        let title = wide(doc_name);
        let di = DOCINFOW {
            cbSize: size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(title.as_ptr()),
            lpszOutput: PCWSTR::null(),
            lpszDatatype: PCWSTR::null(),
            fwType: 0,
        };
        let r = unsafe { StartDocW(hdc, &di) };
        if r <= 0 {
            unsafe {
                let _ = DeleteDC(hdc);
            }
            return Err("gagal memulai dokumen cetak".to_string());
        }
        Ok(PrintJob { hdc })
    }

    /// (dpi_x, dpi_y, lebar_cetak_px, tinggi_cetak_px) area tercetak printer.
    pub fn caps(&self) -> (i32, i32, i32, i32) {
        unsafe {
            (
                GetDeviceCaps(self.hdc, LOGPIXELSX),
                GetDeviceCaps(self.hdc, LOGPIXELSY),
                GetDeviceCaps(self.hdc, HORZRES),
                GetDeviceCaps(self.hdc, VERTRES),
            )
        }
    }

    /// Cetak satu bitmap BGRA (top-down, 32-bit) ke rect tujuan (device px).
    pub fn page(
        &self,
        bgra: &[u8],
        w: i32,
        h: i32,
        dx: i32,
        dy: i32,
        dw: i32,
        dh: i32,
    ) -> Result<(), String> {
        unsafe {
            if StartPage(self.hdc) <= 0 {
                return Err("StartPage gagal".to_string());
            }
            SetStretchBltMode(self.hdc, HALFTONE);
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negatif = top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0 as u32,
                    ..Default::default()
                },
                ..Default::default()
            };
            StretchDIBits(
                self.hdc,
                dx,
                dy,
                dw,
                dh,
                0,
                0,
                w,
                h,
                Some(bgra.as_ptr() as *const _),
                &bi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            if EndPage(self.hdc) <= 0 {
                return Err("EndPage gagal".to_string());
            }
        }
        Ok(())
    }

    /// Selesaikan dokumen & lepaskan device context.
    pub fn finish(self) -> Result<(), String> {
        unsafe {
            EndDoc(self.hdc);
            let _ = DeleteDC(self.hdc);
        }
        Ok(())
    }
}
