//! Qt publication adapter for the shared media-server operation gate.
use qbz_app::settings::{
    media_connection::{Attempt, ConnectionGate},
    media_servers::MediaServerKind,
};
use std::sync::LazyLock;
use std::time::Duration;

static GATE: LazyLock<ConnectionGate> = LazyLock::new(ConnectionGate::default);

pub fn publish() {
    crate::local_bridge::ui(|mut b| {
        // Read at delivery time: an old queued callback cannot restore a stale status.
        let jellyfin = GATE.status(MediaServerKind::Jellyfin);
        let subsonic = GATE.status(MediaServerKind::Subsonic);
        b.as_mut()
            .set_media_syncing(jellyfin.syncing || subsonic.syncing);
        let state = serde_json::json!({ "jellyfin": jellyfin, "subsonic": subsonic });
        b.as_mut()
            .set_media_status(cxx_qt_lib::QString::from(state.to_string().as_str()));
    });
}
pub struct Operation(Option<Attempt>);
impl Operation {
    #[cfg(test)]
    pub fn isolated(kind: MediaServerKind) -> Self {
        Self(Some(
            ConnectionGate::default()
                .begin(kind, "authenticating")
                .unwrap(),
        ))
    }

    pub fn begin(kind: MediaServerKind, phase: &str) -> Option<Self> {
        if crate::media_sync_qt::is_syncing(kind) {
            return None;
        }
        let op = Self(Some(GATE.begin(kind, phase)?));
        log::info!(
            "[media-connection] provider={} phase={phase}",
            kind.as_str()
        );
        publish();
        Some(op)
    }
    fn attempt(&self) -> &Attempt {
        self.0.as_ref().expect("active operation")
    }
    pub fn current(&self) -> bool {
        self.attempt().current()
    }
    pub fn commit<T>(&self, write: impl FnOnce() -> T) -> Option<T> {
        self.attempt().commit(write)
    }
    pub fn phase(&self, phase: &str, error: &str) {
        if !self.current() {
            return;
        }
        log::info!(
            "[media-connection] provider={} phase={phase}",
            self.attempt().kind().as_str()
        );
        self.attempt().phase(phase, error);
        publish();
    }
    pub fn pairing_code(&self, code: &str) {
        self.attempt().pairing_code(code);
        publish();
    }
    pub async fn request<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, String>>,
        timeout: Duration,
    ) -> Option<Result<T, String>> {
        use qbz_app::settings::media_connection::RequestOutcome;
        match self.attempt().run(future, timeout).await {
            RequestOutcome::Completed(result) => Some(result),
            RequestOutcome::Cancelled => None,
            RequestOutcome::TimedOut => Some(Err(qbz_i18n::t(
                "Connection timed out. Check the server address and try again.",
            ))),
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        drop(self.0.take());
        publish();
    }
}
pub fn cancel(kind: MediaServerKind) {
    GATE.cancel(kind);
    publish();
}
pub fn cancel_all() {
    for kind in MediaServerKind::ALL {
        cancel(kind);
    }
}
pub fn syncing(kind: MediaServerKind, active: bool) {
    GATE.sync(kind, active);
    publish();
}
pub fn progress(kind: MediaServerKind, value: String) {
    GATE.progress(kind, value);
    publish();
}

pub fn begin_sync(kind: MediaServerKind) -> bool {
    let acquired = GATE.begin_sync(kind);
    if acquired {
        publish();
    }
    acquired
}

pub fn cancel_pairing() {
    GATE.cancel_pairing(MediaServerKind::Jellyfin);
    publish();
}
