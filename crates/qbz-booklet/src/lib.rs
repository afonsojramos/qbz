//! Album booklet (PDF) rasterization.
//!
//! Frontend-agnostic MuPDF wrapper: open a downloaded booklet PDF, read its
//! page count + per-page sizes, and rasterize a single page to PNG bytes at a
//! given DPI/rotation. The caller owns the download + temp-file lifecycle and
//! runs these (blocking) calls off the UI thread.
//!
//! This is a faithful port of the Tauri `src-tauri/src/pdf_viewer.rs` render
//! path — the working, shipped booklet viewer. The render call deliberately
//! uses `device_rgb()` with `alpha = false` so transparent regions in the PDF
//! composite over an opaque white background instead of rendering black (the
//! rendering bug the Tauri build had to work around).

use std::io::Cursor;
use std::path::Path;

use mupdf::{Colorspace, Document, Matrix};

/// Page size in points (1/72 inch), as MuPDF reports it.
#[derive(Debug, Clone)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// Metadata returned when a booklet PDF is opened.
#[derive(Debug, Clone)]
pub struct BookletInfo {
    pub num_pages: u32,
    pub page_sizes: Vec<PageSize>,
}

/// A single rasterized page, PNG-encoded by MuPDF's own writer.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// PNG image bytes (decode with the `image` crate on the caller side).
    pub png: Vec<u8>,
    /// Pixel dimensions of the rendered image.
    pub width: u32,
    pub height: u32,
}

/// Open a booklet PDF and read its page count + per-page sizes.
///
/// Blocking (MuPDF). Run on a worker thread.
pub fn open_info(path: &Path) -> Result<BookletInfo, String> {
    // Owned String so `&path_str` is `&String` (AsRef<FilePath>), matching the
    // proven Tauri call — `&Cow<str>` does NOT implement AsRef<FilePath>.
    let path_str = path.to_string_lossy().to_string();
    let document =
        Document::open(&path_str).map_err(|e| format!("Failed to open PDF: {e:?}"))?;

    let num_pages = document
        .page_count()
        .map_err(|e| format!("Failed to get page count: {e:?}"))? as u32;

    let mut page_sizes = Vec::with_capacity(num_pages as usize);
    for i in 0..num_pages {
        match document.load_page(i as i32) {
            Ok(page) => {
                let bounds = page
                    .bounds()
                    .map_err(|e| format!("Failed to get page bounds: {e:?}"))?;
                page_sizes.push(PageSize {
                    width: bounds.width(),
                    height: bounds.height(),
                });
            }
            Err(e) => {
                // Fall back to US Letter so a single bad page doesn't sink the
                // whole booklet (mirrors the Tauri behaviour).
                log::warn!("[qbz-booklet] failed to load page {i}: {e:?}");
                page_sizes.push(PageSize {
                    width: 612.0,
                    height: 792.0,
                });
            }
        }
    }

    Ok(BookletInfo {
        num_pages,
        page_sizes,
    })
}

/// Rasterize a single 1-based page to PNG bytes.
///
/// `dpi` is clamped to 36..=600 (0.5x..8.33x). `rotation` is degrees (0/90/
/// 180/270; other values are applied verbatim but only the cardinal rotations
/// get the translate correction that keeps the page in frame). The document is
/// re-opened per call so no non-`Send` MuPDF handle ever crosses threads —
/// exactly how the Tauri viewer does it.
///
/// Blocking (MuPDF). Run on a worker thread.
pub fn render_page_png(
    path: &Path,
    page: u32,
    dpi: u32,
    rotation: u32,
) -> Result<RenderedPage, String> {
    // Owned String so `&path_str` is `&String` (AsRef<FilePath>), matching the
    // proven Tauri call — `&Cow<str>` does NOT implement AsRef<FilePath>.
    let path_str = path.to_string_lossy().to_string();
    let dpi = dpi.clamp(36, 600);

    let document =
        Document::open(&path_str).map_err(|e| format!("Failed to open PDF: {e:?}"))?;

    let page_index = page.saturating_sub(1) as i32;
    let pdf_page = document
        .load_page(page_index)
        .map_err(|e| format!("Failed to load page {page}: {e:?}"))?;

    let scale = dpi as f32 / 72.0;

    let matrix = if rotation % 360 == 0 {
        Matrix::new_scale(scale, scale)
    } else {
        let bounds = pdf_page
            .bounds()
            .map_err(|e| format!("Failed to get bounds: {e:?}"))?;
        let w = bounds.width() * scale;
        let h = bounds.height() * scale;

        let mut m = Matrix::new_scale(scale, scale);
        m.concat(Matrix::new_rotate(rotation as f32));
        match rotation % 360 {
            90 => {
                m.concat(Matrix::new_translate(h, 0.0));
            }
            180 => {
                m.concat(Matrix::new_translate(w, h));
            }
            270 => {
                m.concat(Matrix::new_translate(0.0, w));
            }
            _ => {}
        };
        m
    };

    // device_rgb + alpha=false => opaque white background (the rendering
    // workaround: with alpha, transparent PDF regions rasterize black).
    let pixmap = pdf_page
        .to_pixmap(&matrix, &Colorspace::device_rgb(), false, true)
        .map_err(|e| format!("Failed to render page: {e:?}"))?;

    let width = pixmap.width() as u32;
    let height = pixmap.height() as u32;

    let mut png = Vec::new();
    let mut cursor = Cursor::new(&mut png);
    pixmap
        .write_to(&mut cursor, mupdf::ImageFormat::PNG)
        .map_err(|e| format!("Failed to encode PNG: {e:?}"))?;

    Ok(RenderedPage { png, width, height })
}
