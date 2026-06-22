# Feather PDF

A lightweight, native PDF reader for Windows that stays fast and snappy while
offering a modern, Acrobat-style interface. Built with Tauri 2 and Svelte 5,
rendering through PDFium. Small installer, low memory, no embedded browser shell
beyond the system WebView2.

## Features

- Multi-document tabs, open many PDFs at once
- Page layouts: single page, continuous, facing (two-up), and facing continuous
- Zoom by slider, manual percentage, Ctrl and mouse wheel, fit width or page,
  and presets, all while preserving your reading position
- Thumbnail and outline (bookmarks) sidebars
- Text search with on-page highlights and match-to-match navigation
- Dark mode, plus an invert filter for night reading of page content
- Immersive fullscreen reading mode
- Remembers scroll position per tab; type a page number or zoom level directly
- Drag and drop files to open them
- Printing (print-as-image) through a dedicated dialog:
  - Printer and paper selection, including driver sizes plus a built-in
    F4 / Folio custom size
  - Orientation, copies, page range, odd/even subset, and reverse order
  - Scale (fit to paper, actual size, or custom), grayscale, a live preview,
    and print progress
- Confirmation prompt when closing with multiple documents open

## Tech stack

- Shell: Tauri 2 (Rust backend, system WebView2 frontend)
- UI: Svelte 5, Vite, TypeScript, plain CSS, Lucide icons
- PDF engine: PDFium via `pdfium-render`
- Printing: Windows GDI via the `windows` crate

## Prerequisites

- Node.js and pnpm
- Rust (stable) and the Tauri prerequisites for Windows: Microsoft C++ Build
  Tools and the WebView2 runtime
- `pdfium.dll` is not committed to the repository. Download the Windows x64
  build from
  [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
  and place it at:

  ```
  src-tauri/pdfium.dll
  ```

## Development

```
pnpm install
pnpm tauri dev
```

The dev server runs on port 1440.

## Build

```
pnpm tauri build
```

The installer is produced under `src-tauri/target/release/bundle`.

## Keyboard shortcuts

| Action                 | Shortcut            |
| ---------------------- | ------------------- |
| Open file              | Ctrl+O              |
| Print                  | Ctrl+P              |
| Find                   | Ctrl+F              |
| Close tab              | Ctrl+W              |
| Zoom in / out          | Ctrl+= / Ctrl+-     |
| Fullscreen             | F11                 |
| Close dialog / exit fullscreen | Esc         |

## License

Released under the MIT License. See [LICENSE](LICENSE).

PDFium is a separate component distributed under the BSD 3-Clause License; the
`pdfium.dll` binary is obtained separately as described above.
