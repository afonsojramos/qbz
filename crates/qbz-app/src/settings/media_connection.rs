//! Per-provider single-flight connection state. No credentials or URLs belong here.
use super::media_servers::MediaServerKind;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub busy: bool,
    pub phase: String,
    pub error: String,
    pub syncing: bool,
    pub progress: String,
    pub pairing_code: String,
}
#[derive(Default)]
struct Slot {
    generation: u64,
    status: Status,
    cancel: Option<watch::Sender<bool>>,
}
#[derive(Clone, Default)]
pub struct ConnectionGate(Arc<Mutex<[Slot; 2]>>);
fn index(kind: MediaServerKind) -> usize {
    match kind {
        MediaServerKind::Jellyfin => 0,
        MediaServerKind::Subsonic => 1,
    }
}
pub struct Attempt {
    gate: ConnectionGate,
    kind: MediaServerKind,
    generation: u64,
    cancel: watch::Receiver<bool>,
}
impl ConnectionGate {
    pub fn begin(&self, kind: MediaServerKind, phase: &str) -> Option<Attempt> {
        let mut slots = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(kind)];
        if s.status.busy || s.status.syncing {
            return None;
        }
        s.generation += 1;
        let (tx, rx) = watch::channel(false);
        s.cancel = Some(tx);
        s.status.busy = true;
        s.status.phase = phase.into();
        s.status.error.clear();
        s.status.pairing_code.clear();
        Some(Attempt {
            gate: self.clone(),
            kind,
            generation: s.generation,
            cancel: rx,
        })
    }
    pub fn cancel(&self, kind: MediaServerKind) {
        let mut slots = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(kind)];
        s.generation += 1;
        if let Some(tx) = s.cancel.take() {
            let _ = tx.send(true);
        }
        s.status = Status::default();
    }
    pub fn cancel_pairing(&self, kind: MediaServerKind) {
        let mut slots = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(kind)];
        if s.status.busy && s.status.phase.starts_with("pairing") {
            s.generation += 1;
            if let Some(tx) = s.cancel.take() {
                let _ = tx.send(true);
            }
            s.status = Status::default();
        }
    }
    pub fn status(&self, kind: MediaServerKind) -> Status {
        self.0.lock().unwrap_or_else(|e| e.into_inner())[index(kind)]
            .status
            .clone()
    }
    pub fn begin_sync(&self, kind: MediaServerKind) -> bool {
        let mut slots = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(kind)];
        if s.status.syncing || (s.status.busy && s.status.phase != "syncing") {
            return false;
        }
        s.status.syncing = true;
        true
    }
    pub fn sync(&self, kind: MediaServerKind, syncing: bool) {
        let mut slots = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(kind)];
        s.status.syncing = syncing;
        if !syncing {
            s.status.progress.clear();
        }
    }
    pub fn progress(&self, kind: MediaServerKind, progress: String) {
        self.0.lock().unwrap_or_else(|e| e.into_inner())[index(kind)]
            .status
            .progress = progress;
    }
}
impl Attempt {
    pub fn kind(&self) -> MediaServerKind {
        self.kind
    }
    /// Serializes the final write with cancellation/account changes.
    pub fn commit<T>(&self, write: impl FnOnce() -> T) -> Option<T> {
        let slots = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
        (slots[index(self.kind)].generation == self.generation).then(write)
    }
    pub fn current(&self) -> bool {
        self.commit(|| ()).is_some()
    }
    pub fn phase(&self, phase: &str, error: &str) {
        let mut slots = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(self.kind)];
        if s.generation == self.generation {
            s.status.phase = phase.into();
            s.status.error = error.into();
            if phase != "pairing-waiting" {
                s.status.pairing_code.clear();
            }
        }
    }
    pub fn pairing_code(&self, code: &str) {
        let mut slots = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(self.kind)];
        if s.generation == self.generation {
            s.status.pairing_code = code.into();
        }
    }
    pub async fn cancelled(&mut self) {
        if *self.cancel.borrow() {
            return;
        }
        let _ = self.cancel.changed().await;
    }
    pub async fn run<T, E>(
        &self,
        future: impl std::future::Future<Output = Result<T, E>>,
        limit: std::time::Duration,
    ) -> RequestOutcome<T, E> {
        let mut cancel = self.cancel.clone();
        if !self.current() {
            return RequestOutcome::Cancelled;
        }
        tokio::select! {
            biased;
            _ = cancel.changed() => RequestOutcome::Cancelled,
            result = tokio::time::timeout(limit, future) => {
                if !self.current() { return RequestOutcome::Cancelled; }
                match result {
                    Ok(result) => RequestOutcome::Completed(result),
                    Err(_) => RequestOutcome::TimedOut,
                }
            }
        }
    }
}
pub enum RequestOutcome<T, E> {
    Completed(Result<T, E>),
    Cancelled,
    TimedOut,
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let mut slots = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut slots[index(self.kind)];
        if s.generation == self.generation {
            s.status.busy = false;
            s.status.pairing_code.clear();
            if matches!(
                s.status.phase.as_str(),
                "testing"
                    | "authenticating"
                    | "verifying"
                    | "syncing"
                    | "pairing-start"
                    | "pairing-waiting"
                    | "pairing-verifying"
            ) {
                s.status.phase.clear();
            }
            s.cancel = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_cancellation_clears_code_and_cannot_cancel_password_or_sync() {
        let gate = ConnectionGate::default();
        let kind = MediaServerKind::Jellyfin;
        let old = gate.begin(kind, "pairing-waiting").unwrap();
        old.pairing_code("123456");
        assert!(gate.begin(kind, "authenticating").is_none());
        gate.cancel_pairing(kind);
        assert!(gate.status(kind).pairing_code.is_empty());
        let new = gate.begin(kind, "authenticating").unwrap();
        old.pairing_code("old");
        gate.cancel_pairing(kind);
        assert!(new.current());
        assert!(gate.status(kind).pairing_code.is_empty());
        new.phase("syncing", "");
        gate.cancel_pairing(kind);
        assert!(new.current());
    }
    #[test]
    fn single_flight_per_provider_and_retry() {
        let gate = ConnectionGate::default();
        for kind in MediaServerKind::ALL {
            let first = gate.begin(kind, "authenticating").unwrap();
            assert!(gate.begin(kind, "testing").is_none());
            drop(first);
            assert!(gate.status(kind).phase.is_empty());
            assert!(gate.begin(kind, "authenticating").is_some());
        }
        let _jf = gate
            .begin(MediaServerKind::Jellyfin, "authenticating")
            .unwrap();
        assert!(gate.begin(MediaServerKind::Subsonic, "testing").is_some());
    }
    #[test]
    fn cancelled_completion_cannot_write_or_release_new_attempt() {
        for kind in MediaServerKind::ALL {
            let gate = ConnectionGate::default();
            let old = gate.begin(kind, "authenticating").unwrap();
            gate.cancel(kind);
            let new = gate.begin(kind, "authenticating").unwrap();
            assert!(old.commit(|| panic!("stale credentials written")).is_none());
            old.phase("connected", "old error");
            drop(old);
            assert!(gate.status(kind).busy);
            assert_eq!(gate.status(kind).phase, "authenticating");
            assert!(new.current());
        }
    }
    #[tokio::test]
    async fn cancellation_wakes_pending_request() {
        let gate = ConnectionGate::default();
        let mut attempt = gate.begin(MediaServerKind::Jellyfin, "testing").unwrap();
        gate.cancel(MediaServerKind::Jellyfin);
        tokio::time::timeout(std::time::Duration::from_millis(100), attempt.cancelled())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn timeout_releases_attempt_and_allows_retry() {
        for kind in MediaServerKind::ALL {
            let gate = ConnectionGate::default();
            let attempt = gate.begin(kind, "authenticating").unwrap();
            let out = attempt
                .run(
                    std::future::pending::<Result<(), String>>(),
                    std::time::Duration::from_millis(10),
                )
                .await;
            assert!(matches!(out, RequestOutcome::TimedOut));
            drop(attempt);
            assert!(gate.begin(kind, "authenticating").is_some());
        }
    }
    #[test]
    fn sync_and_sign_in_cannot_overlap() {
        let gate = ConnectionGate::default();
        let attempt = gate
            .begin(MediaServerKind::Jellyfin, "authenticating")
            .unwrap();
        assert!(!gate.begin_sync(MediaServerKind::Jellyfin));
        attempt.phase("syncing", "");
        assert!(gate.begin_sync(MediaServerKind::Jellyfin));
        assert!(!gate.begin_sync(MediaServerKind::Jellyfin));
    }
    #[test]
    fn background_sync_blocks_auth_only_for_same_provider() {
        let gate = ConnectionGate::default();
        gate.sync(MediaServerKind::Jellyfin, true);
        assert!(gate
            .begin(MediaServerKind::Jellyfin, "authenticating")
            .is_none());
        assert!(gate
            .begin(MediaServerKind::Subsonic, "authenticating")
            .is_some());
        gate.sync(MediaServerKind::Jellyfin, false);
        assert!(gate
            .begin(MediaServerKind::Jellyfin, "authenticating")
            .is_some());
    }
}
