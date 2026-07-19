// crates/qbzd/src/cli/art.rs — the `qbzd art` verb (02 §2.3). Current-track
// cover art. Reads the artwork_url already carried on the now-playing track
// (GET /api/now-playing, stamped from the Qobuz image CDN — an unauthenticated
// URL), so it needs no dedicated route in this slice: bare `art` prints the
// URL (pipe into a viewer), `--save PATH` downloads it (notification icons).
// A daemon-served GET /api/artwork/current (302) is deferred until a client
// that can only speak to the daemon needs it.
use crate::cli::client::ApiClient;
use crate::paths::ProfileRoots;

/// `qbzd art [--save PATH]`. Exit 0 · 3 · 4 · 6 (nothing playing / no art).
pub async fn art(host: Option<String>, save: Option<String>, roots: &ProfileRoots) -> i32 {
    let client = ApiClient::new(host, roots);
    let np = match client.get("/api/now-playing").await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return e.exit_code();
        }
    };
    let url = np
        .get("track")
        .and_then(|t| t.get("artwork_url"))
        .and_then(|x| x.as_str())
        .filter(|u| !u.is_empty());
    let url = match url {
        Some(u) => u.to_string(),
        None => {
            eprintln!("error: no artwork for the current track");
            eprintln!("  → is something playing?  qbzd now");
            return 6;
        }
    };

    match save {
        None => {
            println!("{url}");
            0
        }
        Some(path) => match download(&url, &path).await {
            Ok(()) => {
                println!("saved {path}");
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    }
}

async fn download(url: &str, path: &str) -> Result<(), String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("cover download failed: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(path, &bytes).map_err(|e| e.to_string())
}
