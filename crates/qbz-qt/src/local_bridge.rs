//! QbzLocal — the Local Library domain bridge (the phase-23 per-domain
//! pattern, modelled on `home_bridge.rs`: ONE `#[cxx_qt::bridge]` mod, ONE
//! `#[qml_element] #[qml_singleton]` QObject, its own
//! `OnceLock<CxxQtThread>`, a `boot()` invokable and a `pub(crate) fn ui()`).
//!
//! Every property is ONE JSON document (the `library_qt.rs` transport
//! rationale): the QML view parses it once per publish and derives its
//! search / sort / grouping in JS. Artwork never rides the document — it is
//! id-keyed through `localArtworkReady`, windowed by the grid/list, and
//! evicted QML-side, which is what keeps a 16K-track library from decoding
//! covers it never shows.
//!
//! The invokables stay one-line forwards into `local_library_qt.rs`; the
//! blocking DB work runs on `spawn_blocking` so the Qt event loop is never
//! touched by a rusqlite call.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

use crate::local_library_qt as lib;

#[cxx_qt::bridge]
pub mod qbz_local {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Availability / chrome ----------------------------------------
        /// False when there is no per-user library.db or no registered
        /// folder — the view then shows the "nothing indexed yet" state
        /// with the route into Settings > Local Library.
        #[qproperty(bool, local_available)]
        /// {albums, artists, folders, tracks} — the tab badges.
        #[qproperty(QString, local_counts_json)]
        /// Album identity: "folder" | "metadata" (persisted; shared with
        /// the Slint frontend's locallibrary_ui.json).
        #[qproperty(QString, local_album_mode)]

        // --- Albums tab ---------------------------------------------------
        #[qproperty(bool, local_albums_loading)]
        #[qproperty(QString, local_albums_error)]
        #[qproperty(QString, local_albums_json)]

        // --- Artists tab --------------------------------------------------
        #[qproperty(bool, local_artists_loading)]
        #[qproperty(QString, local_artists_json)]

        // --- Folders tab, FLAT mode ---------------------------------------
        #[qproperty(bool, local_folders_loading)]
        #[qproperty(QString, local_folders_json)]

        // --- Folders tab, TREE mode ---------------------------------------
        #[qproperty(bool, local_tree_loading)]
        /// The FLATTENED, search-filtered visible tree — a plain array the
        /// rail windows with a ListView (never a recursive component).
        #[qproperty(QString, local_tree_json)]
        #[qproperty(bool, local_detail_loading)]
        /// {path, name, trackCount, subfolders[], tracks[]}
        #[qproperty(QString, local_detail_json)]

        // --- Tracks tab (server-paginated) --------------------------------
        #[qproperty(bool, local_tracks_loading)]
        #[qproperty(bool, local_tracks_loading_more)]
        #[qproperty(bool, local_tracks_has_more)]
        #[qproperty(QString, local_tracks_sort)]
        #[qproperty(QString, local_tracks_json)]

        // --- Local album detail (the album pane) ---------------------------
        #[qproperty(bool, local_album_loading)]
        /// {album:{...}, tracks:[...]} — "" while nothing is open.
        #[qproperty(QString, local_album_json)]

        type QbzLocal = super::QbzLocalRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzLocal>);

        /// Mount / retry for one tab ("albums" | "artists" | "folders" |
        /// "tracks"). Idempotent per publish: the view calls it on mount and
        /// on tab change; a reload is cheap because every tab is one query.
        #[qinvokable]
        fn load_tab(self: Pin<&mut QbzLocal>, tab: QString);

        /// Album identity dropdown ("folder" | "metadata"): persist + reload
        /// the album set (the grouping IS the query).
        #[qinvokable]
        fn set_album_mode(self: Pin<&mut QbzLocal>, mode: QString);

        // --- Tracks tab ----------------------------------------------------
        /// Toolbar search (server-side; resets to page 1).
        #[qinvokable]
        fn tracks_search(self: Pin<&mut QbzLocal>, query: QString);
        /// Toolbar sort (SQL ORDER BY; resets to page 1).
        #[qinvokable]
        fn tracks_set_sort(self: Pin<&mut QbzLocal>, sort: QString);
        /// Infinite scroll: append the next page.
        #[qinvokable]
        fn tracks_load_more(self: Pin<&mut QbzLocal>);

        // --- Folder tree ---------------------------------------------------
        /// Chevron: expand (lazy one-level fetch) or collapse (pure UI).
        #[qinvokable]
        fn tree_toggle(self: Pin<&mut QbzLocal>, path: QString, expand: bool);
        /// Rail header: collapse every expanded folder.
        #[qinvokable]
        fn tree_collapse_all(self: Pin<&mut QbzLocal>);
        /// Rail search box (filters the visible set, keeps ancestors).
        #[qinvokable]
        fn tree_search(self: Pin<&mut QbzLocal>, query: QString);
        /// Row body: select a folder and load its detail pane.
        #[qinvokable]
        fn select_folder(self: Pin<&mut QbzLocal>, path: QString);

        // --- Album detail ---------------------------------------------------
        /// Album card click: load the local album detail (header + tracks).
        #[qinvokable]
        fn open_album(self: Pin<&mut QbzLocal>, id: QString);
        /// Close the album pane (back to the grid).
        #[qinvokable]
        fn close_album(self: Pin<&mut QbzLocal>);

        // --- Playback --------------------------------------------------------
        /// Album card / detail header Play (optionally shuffled).
        #[qinvokable]
        fn play_album(self: Pin<&mut QbzLocal>, id: QString, shuffle: bool);
        /// Album-detail row click: play the album from that track.
        #[qinvokable]
        fn play_album_track(self: Pin<&mut QbzLocal>, id: QString, track_id: QString);
        /// Tree detail header Play: the whole subtree becomes the queue.
        #[qinvokable]
        fn play_folder(self: Pin<&mut QbzLocal>, path: QString);
        /// Tree detail row click: the folder's DIRECT tracks become the
        /// queue, starting at the clicked row.
        #[qinvokable]
        fn play_folder_track(self: Pin<&mut QbzLocal>, path: QString, track_id: QString);
        /// Tracks-tab row click: the loaded page set becomes the queue.
        #[qinvokable]
        fn play_track(self: Pin<&mut QbzLocal>, track_id: QString);
        /// Context menus: kind = "track" | "album" | "folder";
        /// mode = "next" | "later" | "queue".
        #[qinvokable]
        fn enqueue(self: Pin<&mut QbzLocal>, kind: QString, id: QString, mode: QString);

        // --- Windowed artwork -------------------------------------------------
        /// The mounted window reports its artKeys; Rust resolves each to a
        /// 256px thumbnail and emits `localArtworkReady` per hit.
        #[qinvokable]
        fn artwork_window(self: Pin<&mut QbzLocal>, keys_json: QString);
        /// (key, "file://…") — id-keyed so a cover can never land on the
        /// wrong row.
        #[qsignal]
        fn local_artwork_ready(self: Pin<&mut QbzLocal>, key: QString, path: QString);
    }

    impl cxx_qt::Threading for QbzLocal {}
}

use qbz_local::QbzLocal;

/// Rust side of the local-library bridge (plain storage, phase-1 pattern).
pub struct QbzLocalRust {
    local_available: bool,
    local_counts_json: QString,
    local_album_mode: QString,
    local_albums_loading: bool,
    local_albums_error: QString,
    local_albums_json: QString,
    local_artists_loading: bool,
    local_artists_json: QString,
    local_folders_loading: bool,
    local_folders_json: QString,
    local_tree_loading: bool,
    local_tree_json: QString,
    local_detail_loading: bool,
    local_detail_json: QString,
    local_tracks_loading: bool,
    local_tracks_loading_more: bool,
    local_tracks_has_more: bool,
    local_tracks_sort: QString,
    local_tracks_json: QString,
    local_album_loading: bool,
    local_album_json: QString,
}

impl Default for QbzLocalRust {
    fn default() -> Self {
        Self {
            local_available: false,
            local_counts_json: QString::from("{}"),
            local_album_mode: QString::from("folder"),
            local_albums_loading: false,
            local_albums_error: QString::default(),
            local_albums_json: QString::from("[]"),
            local_artists_loading: false,
            local_artists_json: QString::from("[]"),
            local_folders_loading: false,
            local_folders_json: QString::from("[]"),
            local_tree_loading: false,
            local_tree_json: QString::from("[]"),
            local_detail_loading: false,
            local_detail_json: QString::from(""),
            local_tracks_loading: false,
            local_tracks_loading_more: false,
            local_tracks_has_more: false,
            local_tracks_sort: QString::from("default"),
            local_tracks_json: QString::from("[]"),
            local_album_loading: false,
            local_album_json: QString::from(""),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzLocal>> = OnceLock::new();

/// Queue a local-bridge mutation onto the Qt event loop (no-op before boot
/// registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzLocal>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// Republish the tab badges + availability after any load.
fn publish_counts() {
    let counts = lib::to_json(&lib::counts());
    ui(move |mut b| {
        b.as_mut()
            .set_local_counts_json(QString::from(counts.as_str()));
    });
}

fn publish_tree(nodes_json: String) {
    ui(move |mut b| {
        b.as_mut()
            .set_local_tree_json(QString::from(nodes_json.as_str()));
        b.as_mut().set_local_tree_loading(false);
    });
}

impl qbz_local::QbzLocal {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] local Qt thread already registered");
        }
        // Seed the persisted toolbar choices so the first paint matches
        // what the user last picked (shared with the Slint frontend).
        let mode = lib::album_mode();
        let sort = lib::tracks_sort();
        crate::spawn(async move {
            let available = tokio::task::spawn_blocking(lib::has_library)
                .await
                .unwrap_or(false);
            ui(move |mut b| {
                b.as_mut().set_local_available(available);
                b.as_mut()
                    .set_local_album_mode(QString::from(mode.as_str()));
                b.as_mut()
                    .set_local_tracks_sort(QString::from(sort.as_str()));
            });
        });
    }

    pub fn load_tab(self: Pin<&mut Self>, tab: QString) {
        load_tab_impl(tab.to_string());
    }

    pub fn set_album_mode(self: Pin<&mut Self>, mode: QString) {
        let mode = mode.to_string();
        lib::set_album_mode(&mode);
        let published = lib::album_mode();
        ui(move |mut b| {
            b.as_mut()
                .set_local_album_mode(QString::from(published.as_str()));
        });
        // The grouping IS the query — reload both album surfaces.
        load_tab_impl("albums".to_string());
    }

    pub fn tracks_search(self: Pin<&mut Self>, query: QString) {
        lib::set_tracks_query(&query.to_string());
        load_tracks(true);
    }

    pub fn tracks_set_sort(self: Pin<&mut Self>, sort: QString) {
        let sort = sort.to_string();
        lib::set_tracks_sort(&sort);
        ui(move |mut b| {
            b.as_mut()
                .set_local_tracks_sort(QString::from(sort.as_str()));
        });
        load_tracks(true);
    }

    pub fn tracks_load_more(self: Pin<&mut Self>) {
        if !lib::tracks_has_more() {
            return;
        }
        load_tracks(false);
    }

    pub fn tree_toggle(self: Pin<&mut Self>, path: QString, expand: bool) {
        let path = path.to_string();
        if !expand {
            lib::tree_collapse(&path);
            publish_tree(lib::to_json(&lib::tree_visible()));
            return;
        }
        crate::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || lib::tree_expand_blocking(&path)).await;
            publish_tree(lib::to_json(&lib::tree_visible()));
        });
    }

    pub fn tree_collapse_all(self: Pin<&mut Self>) {
        lib::tree_collapse_all();
        publish_tree(lib::to_json(&lib::tree_visible()));
    }

    pub fn tree_search(self: Pin<&mut Self>, query: QString) {
        lib::set_tree_search(&query.to_string());
        publish_tree(lib::to_json(&lib::tree_visible()));
    }

    pub fn select_folder(self: Pin<&mut Self>, path: QString) {
        let path = path.to_string();
        ui(|mut b| b.as_mut().set_local_detail_loading(true));
        crate::spawn(async move {
            let detail = tokio::task::spawn_blocking(move || lib::load_folder_detail_blocking(&path))
                .await
                .ok();
            let json = detail.map(|d| lib::to_json(&d)).unwrap_or_default();
            ui(move |mut b| {
                b.as_mut()
                    .set_local_detail_json(QString::from(json.as_str()));
                b.as_mut().set_local_detail_loading(false);
            });
        });
    }

    pub fn open_album(self: Pin<&mut Self>, id: QString) {
        let id = id.to_string();
        ui(|mut b| b.as_mut().set_local_album_loading(true));
        crate::spawn(async move {
            let detail = tokio::task::spawn_blocking(move || lib::load_album_detail_blocking(&id))
                .await
                .ok()
                .flatten();
            let json = detail.map(|d| lib::to_json(&d)).unwrap_or_default();
            ui(move |mut b| {
                b.as_mut()
                    .set_local_album_json(QString::from(json.as_str()));
                b.as_mut().set_local_album_loading(false);
            });
        });
    }

    pub fn close_album(self: Pin<&mut Self>) {
        ui(|mut b| {
            b.as_mut().set_local_album_json(QString::from(""));
            b.as_mut().set_local_album_loading(false);
        });
    }

    pub fn play_album(self: Pin<&mut Self>, id: QString, shuffle: bool) {
        let id = id.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_album(&runtime, id, None, shuffle).await;
        });
    }

    pub fn play_album_track(self: Pin<&mut Self>, id: QString, track_id: QString) {
        let id = id.to_string();
        let row = track_id.to_string().parse::<i64>().ok();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_album(&runtime, id, row, false).await;
        });
    }

    pub fn play_folder(self: Pin<&mut Self>, path: QString) {
        let path = path.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_folder(&runtime, path).await;
        });
    }

    pub fn play_folder_track(self: Pin<&mut Self>, path: QString, track_id: QString) {
        let path = path.to_string();
        let Ok(row) = track_id.to_string().parse::<i64>() else {
            return;
        };
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_folder_track(&runtime, path, row).await;
        });
    }

    pub fn play_track(self: Pin<&mut Self>, track_id: QString) {
        let Ok(row) = track_id.to_string().parse::<i64>() else {
            return;
        };
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_tracks_from(&runtime, row).await;
        });
    }

    pub fn enqueue(self: Pin<&mut Self>, kind: QString, id: QString, mode: QString) {
        let (kind, id, mode) = (kind.to_string(), id.to_string(), mode.to_string());
        let runtime = crate::app();
        crate::spawn(async move {
            lib::enqueue(&runtime, kind, id, mode).await;
        });
    }

    pub fn artwork_window(self: Pin<&mut Self>, keys_json: QString) {
        let keys: Vec<String> = serde_json::from_str(&keys_json.to_string()).unwrap_or_default();
        if keys.is_empty() {
            return;
        }
        crate::spawn(async move {
            let hits = tokio::task::spawn_blocking(move || lib::artwork_window_blocking(keys))
                .await
                .unwrap_or_default();
            for (key, path) in hits {
                ui(move |mut b| {
                    b.as_mut().local_artwork_ready(
                        QString::from(key.as_str()),
                        QString::from(path.as_str()),
                    );
                });
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Loaders (each publishes its own tab's document + the shared badges)
// ---------------------------------------------------------------------------

fn load_tab_impl(tab: String) {
    match tab.as_str() {
        "albums" => load_albums(),
        "artists" => load_artists(),
        "folders" => load_folders(),
        "tracks" => load_tracks(true),
        _ => log::warn!("[qbz-qt] local: unknown tab {tab}"),
    }
}

fn load_albums() {
    ui(|mut b| {
        b.as_mut().set_local_albums_loading(true);
        b.as_mut().set_local_albums_error(QString::from(""));
    });
    crate::spawn(async move {
        let result = tokio::task::spawn_blocking(lib::load_albums_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let _ = tokio::task::spawn_blocking(lib::load_counts_blocking).await;
        match result {
            Ok(rows) => {
                let json = lib::to_json(&rows);
                log::info!("[qbz-qt] local albums published: {} ({} bytes)", rows.len(), json.len());
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_albums_json(QString::from(json.as_str()));
                    b.as_mut().set_local_albums_loading(false);
                });
            }
            Err(e) => {
                log::warn!("[qbz-qt] local albums load failed: {e}");
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_albums_error(QString::from(e.as_str()));
                    b.as_mut().set_local_albums_loading(false);
                });
            }
        }
        publish_counts();
    });
}

fn load_artists() {
    ui(|mut b| b.as_mut().set_local_artists_loading(true));
    crate::spawn(async move {
        let rows = tokio::task::spawn_blocking(lib::load_artists_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
            .unwrap_or_default();
        let json = lib::to_json(&rows);
        ui(move |mut b| {
            b.as_mut()
                .set_local_artists_json(QString::from(json.as_str()));
            b.as_mut().set_local_artists_loading(false);
        });
        publish_counts();
    });
}

fn load_folders() {
    ui(|mut b| {
        b.as_mut().set_local_folders_loading(true);
        b.as_mut().set_local_tree_loading(true);
    });
    crate::spawn(async move {
        let rows = tokio::task::spawn_blocking(lib::load_folders_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
            .unwrap_or_default();
        let json = lib::to_json(&rows);
        ui(move |mut b| {
            b.as_mut()
                .set_local_folders_json(QString::from(json.as_str()));
            b.as_mut().set_local_folders_loading(false);
        });
        // The tree rail seeds from the registered library folders; the
        // levels below it are fetched on expand.
        let _ = tokio::task::spawn_blocking(lib::load_tree_roots_blocking).await;
        publish_tree(lib::to_json(&lib::tree_visible()));
        publish_counts();
    });
}

fn load_tracks(reset: bool) {
    ui(move |mut b| {
        if reset {
            b.as_mut().set_local_tracks_loading(true);
        } else {
            b.as_mut().set_local_tracks_loading_more(true);
        }
    });
    crate::spawn(async move {
        let result = tokio::task::spawn_blocking(move || lib::load_tracks_page_blocking(reset))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let _ = tokio::task::spawn_blocking(lib::load_counts_blocking).await;
        match result {
            Ok((rows, has_more)) => {
                let json = lib::to_json(&rows);
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_tracks_json(QString::from(json.as_str()));
                    b.as_mut().set_local_tracks_has_more(has_more);
                    b.as_mut().set_local_tracks_loading(false);
                    b.as_mut().set_local_tracks_loading_more(false);
                });
            }
            Err(e) => {
                log::warn!("[qbz-qt] local tracks load failed: {e}");
                ui(|mut b| {
                    b.as_mut().set_local_tracks_loading(false);
                    b.as_mut().set_local_tracks_loading_more(false);
                });
            }
        }
        publish_counts();
    });
}
