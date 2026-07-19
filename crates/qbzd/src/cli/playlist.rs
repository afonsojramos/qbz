// crates/qbzd/src/cli/playlist.rs — the `qbzd playlist list|show` verbs
// (02 §2.3). Reads only in this slice. `playlist show --ids` emits the
// playlist's track ids (pipe into `queue add -`).
use serde_json::Value;

use crate::cli::browse::{collect_ids, render};
use crate::cli::client::ApiClient;
use crate::paths::ProfileRoots;

/// `qbzd playlist list [--json]`.
pub async fn list(host: Option<String>, json: bool, roots: &ProfileRoots) -> i32 {
    let client = ApiClient::new(host, roots);
    match client.get("/api/playlists").await {
        Ok(v) => {
            if json {
                println!("{}", serde_json::to_string(&v).unwrap_or_default());
            } else {
                print!("{}", render_list(&v));
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            e.exit_code()
        }
    }
}

/// `qbzd playlist show <ID> [--ids] [--json]`.
pub async fn show(host: Option<String>, id: u64, ids: bool, json: bool, roots: &ProfileRoots) -> i32 {
    let client = ApiClient::new(host, roots);
    match client.get(&format!("/api/playlist?id={id}")).await {
        Ok(v) => {
            if json {
                println!("{}", serde_json::to_string(&v).unwrap_or_default());
            } else if ids {
                for tid in collect_ids(&v) {
                    println!("{tid}");
                }
            } else {
                print!("{}", render(&v));
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            e.exit_code()
        }
    }
}

// ============================ internals ============================

/// The collection view: `id  Name (N tracks)` per playlist. The array is under
/// `playlists` (not an items/tracks key), so it has its own small renderer.
fn render_list(v: &Value) -> String {
    match v.get("playlists").and_then(|p| p.as_array()) {
        Some(a) if !a.is_empty() => {
            let mut out = String::new();
            for pl in a {
                let id = pl.get("id").and_then(|x| x.as_u64()).map(|n| n.to_string()).unwrap_or_default();
                let name = pl.get("name").and_then(|x| x.as_str()).unwrap_or("(untitled)");
                let count = pl.get("tracks_count").and_then(|x| x.as_u64()).unwrap_or(0);
                out.push_str(&format!("{id}  {name} ({count} tracks)\n"));
            }
            out
        }
        _ => "no playlists\n".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_list_shows_id_name_count() {
        let v = serde_json::json!({"playlists": [
            {"id": 987, "name": "Fusion", "tracks_count": 42},
            {"id": 12, "name": "Chill"}
        ]});
        let out = render_list(&v);
        assert!(out.contains("987  Fusion (42 tracks)"), "{out}");
        assert!(out.contains("12  Chill (0 tracks)"), "{out}");
    }

    #[test]
    fn render_list_empty() {
        assert_eq!(render_list(&serde_json::json!({"playlists": []})), "no playlists\n");
    }
}
