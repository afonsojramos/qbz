//! Album booklet (digital liner-notes PDF) reader controller.
//!
//! 1:1 port of Tauri's BookletViewer.svelte + `src-tauri/src/pdf_viewer.rs`
//! flow, split across the frontend-agnostic `qbz-booklet` crate (the MuPDF
//! rasterizer, ADR-006) and this thin UI-thread glue:
//!
//!   1. The album controller stashes the booklet goody URL via
//!      [`set_current_url`] when an album loads.
//!   2. [`open`] downloads that PDF to a temp file, reads its page count, and
//!      rasterizes page 1, pushing the decoded image into `BookletState`.
//!   3. Navigation / zoom / rotate re-rasterize the current page off the UI
//!      thread; [`download`] copies the temp PDF to a user-chosen location;
//!      [`close`] removes the temp file and clears state.
//!
//! All network + MuPDF work runs on the tokio runtime / `spawn_blocking`; only
//! the `slint::Image` build + state writes touch the event loop. The reader's
//! mutable state lives in a `thread_local` touched exclusively on the Slint
//! event loop — every worker-thread result is funnelled back through
//! `upgrade_in_event_loop` before it reads/writes that state.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use slint::ComponentHandle;

use crate::{AppWindow, BookletState};

/// Baseline render DPI at zoom 1.0. The qbz-booklet crate clamps to 36..=600,
/// so the zoom range (0.5..4.0) maps to 75..600 DPI.
const BASE_DPI: f32 = 150.0;
/// PDF download timeout, matching the Tauri viewer's client.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// UI-thread reader state. `thread_local` because every field is only ever
/// touched on the Slint event loop (the goody URL is stashed there by
/// `album::apply_album`, and every render is kicked from a callback that hops
/// back to the event loop before reading it).
struct Reader {
    /// The booklet goody URL of the open album ("" = no booklet).
    url: String,
    /// The downloaded temp PDF, once `open` has fetched it.
    path: Option<PathBuf>,
    num_pages: u32,
    /// 1-based current page.
    current_page: u32,
    zoom: f32,
    /// Cumulative rotation in degrees (0/90/180/270).
    rotation: u32,
}

impl Reader {
    const fn new() -> Self {
        Self {
            url: String::new(),
            path: None,
            num_pages: 0,
            current_page: 1,
            zoom: 1.0,
            rotation: 0,
        }
    }
}

thread_local! {
    static READER: RefCell<Reader> = const { RefCell::new(Reader::new()) };
}

/// Stash the booklet goody URL for the currently-open album. Called from
/// `album::apply_album`; cleared by `clear_current_url` on album reset.
pub fn set_current_url(url: &str) {
    READER.with(|cell| cell.borrow_mut().url = url.to_string());
}

/// Clear the stashed booklet URL (album reset). Does NOT remove a downloaded
/// temp file — that is `close`'s job once the reader is actually opened.
pub fn clear_current_url() {
    READER.with(|cell| cell.borrow_mut().url.clear());
}

/// DPI for the current zoom, pre-clamp (qbz-booklet re-clamps to 36..=600).
fn dpi_for(zoom: f32) -> u32 {
    (BASE_DPI * zoom).round() as u32
}

/// Temp dir for downloaded booklets (mirrors Tauri's `qbz-booklets`).
fn booklet_temp_dir() -> PathBuf {
    std::env::temp_dir().join("qbz-booklets")
}

/// Open the booklet reader for the current album. No-op when no booklet URL is
/// stashed. Sets the modal open + loading, then downloads the PDF, reads its
/// page info, and renders page 1. The booklet flow is a direct PDF download
/// (no Qobuz core call), so it takes no `runtime` — unlike the other
/// media-action controllers.
pub fn open(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    // Read the stashed URL + reset the per-open view state on the UI thread.
    let url = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        r.current_page = 1;
        r.zoom = 1.0;
        r.rotation = 0;
        r.path = None;
        r.num_pages = 0;
        r.url.clone()
    });
    if url.is_empty() {
        return;
    }

    let _ = weak.upgrade_in_event_loop(|w| {
        // Header title = the open album's title (the modal binds
        // BookletState.title; nothing in the .slint sets it).
        let title = w.global::<crate::AlbumState>().get_title();
        let st = w.global::<BookletState>();
        st.set_title(title);
        st.set_error(false);
        st.set_error_text("".into());
        st.set_loading(true);
        st.set_current_page(1);
        st.set_num_pages(0);
        st.set_zoom(1.0);
        st.set_rotation(0);
        st.set_open(true);
    });

    let render_handle = handle.clone();
    handle.spawn(async move {
        match download_and_open(&url).await {
            Ok((path, num_pages)) => {
                // Hop back to the UI thread to record the path + page count,
                // then kick the first render (which itself hops off-thread to
                // rasterize, then back to paint).
                let render_handle = render_handle.clone();
                let _ = weak.upgrade_in_event_loop(move |w| {
                    READER.with(|cell| {
                        let mut r = cell.borrow_mut();
                        r.path = Some(path);
                        r.num_pages = num_pages;
                    });
                    w.global::<BookletState>().set_num_pages(num_pages as i32);
                    render_current(w.as_weak(), render_handle);
                });
            }
            Err(e) => {
                log::warn!("[qbz-slint] booklet open failed: {e}");
                let _ = weak.upgrade_in_event_loop(move |w| {
                    let st = w.global::<BookletState>();
                    st.set_loading(false);
                    st.set_error(true);
                    st.set_error_text(e.into());
                });
            }
        }
    });
}

/// Download the PDF at `url` to a fresh temp file, then read its page count.
/// Returns the temp path + page count. Blocking MuPDF work is on
/// `spawn_blocking`. Errors are user-facing strings (shown in the modal).
async fn download_and_open(url: &str) -> Result<(PathBuf, u32), String> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| qbz_i18n::t_args("Failed to fetch booklet: {}", &[&e.to_string()]))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| qbz_i18n::t_args("Failed to read booklet: {}", &[&e.to_string()]))?;

    let temp_dir = booklet_temp_dir();
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| qbz_i18n::t_args("Failed to create temp dir: {}", &[&e.to_string()]))?;
    let path = temp_dir.join(format!("{}.pdf", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| qbz_i18n::t_args("Failed to write booklet: {}", &[&e.to_string()]))?;

    let info_path = path.clone();
    let info = tokio::task::spawn_blocking(move || qbz_booklet::open_info(&info_path))
        .await
        .map_err(|e| qbz_i18n::t_args("Booklet open task failed: {}", &[&e.to_string()]))??;
    Ok((path, info.num_pages))
}

/// Rasterize the current page (off-thread) and paint it into `BookletState`.
/// Reads the page/zoom/rotation snapshot from `READER` on the UI thread, sets
/// `loading`, then hops off to MuPDF and back. No-op when no PDF is open.
fn render_current(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let Some((path, page, dpi, rotation, num_pages, zoom, rot)) = READER.with(|cell| {
        let r = cell.borrow();
        let path = r.path.clone()?;
        Some((
            path,
            r.current_page,
            dpi_for(r.zoom),
            r.rotation,
            r.num_pages,
            r.zoom,
            r.rotation,
        ))
    }) else {
        return;
    };

    // Reflect the snapshot the render is for, and show the spinner.
    if let Some(w) = weak.upgrade() {
        let st = w.global::<BookletState>();
        st.set_loading(true);
        st.set_current_page(page as i32);
        st.set_num_pages(num_pages as i32);
        st.set_zoom(zoom);
        st.set_rotation(rot as i32);
    }

    handle.spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            render_page_rgba(&path, page, dpi, rotation)
        })
        .await;
        match result {
            Ok(Ok((pixels, width, height))) => {
                let _ = weak.upgrade_in_event_loop(move |w| {
                    let image = crate::artwork::pixels_to_image(&pixels, width, height);
                    let st = w.global::<BookletState>();
                    st.set_page_image(image);
                    st.set_page_pixel_width(width as i32);
                    st.set_page_pixel_height(height as i32);
                    st.set_loading(false);
                    st.set_error(false);
                });
            }
            Ok(Err(e)) => {
                log::warn!("[qbz-slint] booklet render failed: {e}");
                let _ = weak.upgrade_in_event_loop(move |w| {
                    let st = w.global::<BookletState>();
                    st.set_loading(false);
                    st.set_error(true);
                    st.set_error_text(e.into());
                });
            }
            Err(e) => {
                log::warn!("[qbz-slint] booklet render task failed: {e}");
                let _ = weak.upgrade_in_event_loop(|w| {
                    w.global::<BookletState>().set_loading(false);
                });
            }
        }
    });
}

/// Rasterize one page to RGBA8 pixels. Blocking (MuPDF + PNG decode); call from
/// `spawn_blocking`. The qbz-booklet crate emits PNG bytes, decoded here with
/// the `image` crate (mirrors `album::apply_artwork`'s RGBA path).
fn render_page_rgba(
    path: &Path,
    page: u32,
    dpi: u32,
    rotation: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let rendered = qbz_booklet::render_page_png(path, page, dpi, rotation)?;
    let rgba = image::load_from_memory(&rendered.png)
        .map_err(|e| qbz_i18n::t_args("Failed to decode booklet page: {}", &[&e.to_string()]))?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((rgba.into_raw(), width, height))
}

/// Advance to the next page (clamped to `num_pages`) and re-render.
pub fn next_page(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let changed = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        if r.num_pages > 0 && r.current_page < r.num_pages {
            r.current_page += 1;
            true
        } else {
            false
        }
    });
    if changed {
        render_current(weak, handle);
    }
}

/// Step back to the previous page (clamped to 1) and re-render.
pub fn prev_page(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let changed = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        if r.current_page > 1 {
            r.current_page -= 1;
            true
        } else {
            false
        }
    });
    if changed {
        render_current(weak, handle);
    }
}

/// Zoom in (x1.25, capped at 4.0) and re-render at the higher DPI.
pub fn zoom_in(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let changed = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        let next = (r.zoom * 1.25).min(4.0);
        if (next - r.zoom).abs() > f32::EPSILON {
            r.zoom = next;
            true
        } else {
            false
        }
    });
    if changed {
        render_current(weak, handle);
    }
}

/// Zoom out (/1.25, floored at 0.5) and re-render at the lower DPI.
pub fn zoom_out(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let changed = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        let next = (r.zoom / 1.25).max(0.5);
        if (next - r.zoom).abs() > f32::EPSILON {
            r.zoom = next;
            true
        } else {
            false
        }
    });
    if changed {
        render_current(weak, handle);
    }
}

/// Reset zoom to the fit-width baseline (1.0) and re-render.
pub fn fit_width(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let changed = READER.with(|cell| {
        let mut r = cell.borrow_mut();
        if (r.zoom - 1.0).abs() > f32::EPSILON {
            r.zoom = 1.0;
            true
        } else {
            false
        }
    });
    if changed {
        render_current(weak, handle);
    }
}

/// Rotate the page by 90° (cumulative, mod 360) and re-render.
pub fn rotate(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    READER.with(|cell| {
        let mut r = cell.borrow_mut();
        r.rotation = (r.rotation + 90) % 360;
    });
    render_current(weak, handle);
}

/// Save the open booklet PDF to a user-chosen location (best-effort). Opens a
/// native save dialog seeded with the album title, then copies the temp file.
pub fn download(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let path = READER.with(|cell| cell.borrow().path.clone());
    let Some(path) = path else {
        return;
    };
    let default_name = weak
        .upgrade()
        .map(|w| {
            let title = w.global::<BookletState>().get_title().to_string();
            if title.is_empty() {
                "booklet.pdf".to_string()
            } else {
                format!("{title}.pdf")
            }
        })
        .unwrap_or_else(|| "booklet.pdf".to_string());

    handle.spawn(async move {
        let Some(dest) = rfd::AsyncFileDialog::new()
            .set_file_name(&default_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
            .await
        else {
            return;
        };
        if let Err(e) = tokio::fs::copy(&path, dest.path()).await {
            log::warn!("[qbz-slint] booklet download copy failed: {e}");
        }
    });
}

/// Close the reader: hide the modal, remove the temp PDF (best-effort), and
/// clear the per-open view state. Runs on the UI thread (the callback site).
pub fn close(window: &AppWindow) {
    let st = window.global::<BookletState>();
    st.set_open(false);
    st.set_loading(false);
    st.set_error(false);
    st.set_error_text("".into());
    st.set_page_image(slint::Image::default());
    st.set_page_pixel_width(0);
    st.set_page_pixel_height(0);
    st.set_current_page(1);
    st.set_num_pages(0);
    st.set_zoom(1.0);
    st.set_rotation(0);

    READER.with(|cell| {
        let mut r = cell.borrow_mut();
        if let Some(path) = r.path.take() {
            let _ = std::fs::remove_file(&path);
        }
        r.num_pages = 0;
        r.current_page = 1;
        r.zoom = 1.0;
        r.rotation = 0;
    });
}
