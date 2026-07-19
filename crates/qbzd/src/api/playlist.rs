// crates/qbzd/src/api/playlist.rs — playlists (02 §2.3, §3.4 row 24). GET
// /api/playlists (the user's collection) and GET /api/playlist?id= (one
// playlist with its COMPLETE track list — get_playlist auto-pages server-side).
// Reads only in this slice; playlist CRUD (create/update/delete/tracks) is a
// later batch. Auth-gated; typed serde shapes verbatim.
use std::io::Cursor;

use serde_json::Value;
use tiny_http::Response;

use crate::state::AuthState;

use super::{err_json, json, ApiState};

/// `GET /api/playlists` — the user's playlist collection.
pub fn list(state: &ApiState) -> Response<Cursor<Vec<u8>>> {
    if let Some(resp) = auth_gate(state) {
        return resp;
    }
    match state.rt.block_on(state.runtime.core().get_user_playlists()) {
        Ok(pls) => json(
            200,
            serde_json::json!({"playlists": serde_json::to_value(pls).unwrap_or(Value::Null)}),
        ),
        Err(_) => err_json(502, "playlists_failed", "playlists request to Qobuz failed", "try again in a moment"),
    }
}

/// `GET /api/playlist?id=<ID>` — one playlist with its full track list.
pub fn show(state: &ApiState, query: &str) -> Response<Cursor<Vec<u8>>> {
    if let Some(resp) = auth_gate(state) {
        return resp;
    }
    let id = match id_param(query) {
        Some(id) => id,
        None => return err_json(400, "bad_request", "playlist requires a numeric id", "usage: qbzd playlist show <ID>"),
    };
    match state.rt.block_on(state.runtime.core().get_playlist(id)) {
        Ok(pl) => json(
            200,
            serde_json::json!({"playlist": serde_json::to_value(pl).unwrap_or(Value::Null)}),
        ),
        Err(_) => err_json(404, "not_found", &format!("playlist {id} not found"), "check: qbzd playlist list"),
    }
}

// ============================ internals ============================

fn id_param(query: &str) -> Option<u64> {
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some("id") {
            return kv.next().and_then(|v| v.parse::<u64>().ok());
        }
    }
    None
}

fn auth_gate(state: &ApiState) -> Option<Response<Cursor<Vec<u8>>>> {
    let needs_auth = state
        .shared
        .lock()
        .map(|s| s.auth == AuthState::NeedsAuth)
        .unwrap_or(false);
    if needs_auth {
        Some(err_json(409, "needs_auth", "not logged in to Qobuz", "run: qbzd login"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_param_reads_numeric_id() {
        assert_eq!(id_param("id=987654"), Some(987654));
        assert_eq!(id_param("foo=1&id=42"), Some(42));
        assert_eq!(id_param("id=abc"), None);
        assert_eq!(id_param(""), None);
    }
}
