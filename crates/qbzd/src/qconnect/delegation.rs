//! qbzd host for the shared transactional QConnect delegation coordinator.
//!
//! Candidate API/QWS work is completed against isolated contexts while the
//! installed runtime remains authoritative. The synchronous commit methods do
//! only a short stamped runtime swap; teardown, queue restoration and cloud
//! bootstrap happen afterwards.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use qbz_app::shell::AppRuntime;
use qbz_core::QueueAuthoritySnapshot;
use qbz_models::{CoreEvent, Quality};
use qbz_qobuz::{DelegatedApiConfig, DelegatedApiEndpoint, DelegatedQobuzClient, QobuzClient};
use qconnect_app::renderer::PLAYING_STATE_STOPPED;
use qconnect_app::{
    CommitRejected, DelegationCancellation, DelegationCoordinator, DelegationErrorCode,
    DelegationHost, QconnectApp, QconnectAppEvent, QconnectEventSink, QconnectLifecycleState,
    QconnectRemoteSyncState, QconnectSessionState, RendererBufferState, RendererReport,
    RendererReportType, RestoreReason, SessionLoopHost, JOIN_SESSION_REASON_CONTROLLER_REQUEST,
};
use qconnect_lan::{HandoffCandidate, LanProjection};
use qconnect_protocol::RendererCommandType;
use qconnect_transport_ws::{NativeWsTransport, TransportEvent, WsTransportConfig};
use tokio::sync::{broadcast, oneshot, watch, Mutex as AsyncMutex, OwnedMutexGuard};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::adapter::DaemonAdapter;
use crate::state::DaemonShared;

use super::authority::{AuthorityCell, AuthorityOrigin, AuthorityStamp};
use super::engine::{DaemonRendererEngine, VolumeMode};
use super::lan::DaemonLanProjectionSlot;
use super::session::{bootstrap_prepared_owner_presence, DaemonSessionLoopHost};
use super::sink::{DaemonEventSink, DaemonQconnectApp};
use super::transport::{default_qconnect_device_info_with_name, resolve_transport_config};
use super::{
    lock_inner, update_lifecycle_state_if_running, DaemonQconnectInner, DaemonQconnectRuntime,
};

type Runtime = Arc<AppRuntime<DaemonAdapter>>;
type SharedState = Arc<StdMutex<DaemonShared>>;
type OwnerRestoreResult = Result<(), &'static str>;
pub type DaemonDelegationCoordinator = DelegationCoordinator<DaemonDelegationHost>;

const PREFLIGHT_BUFFER_EVENTS: usize = 256;
const PREFLIGHT_BUFFER_BYTES: usize = 256 * 1024;
const DELEGATED_REJOIN_SESSION_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_RESTORE_OWNER: u8 = 0;
const SHUTDOWN_DISCARD_OWNER: u8 = 1;
static NEXT_OWNER_RESTORE_TASK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct OwnerRestoreTask {
    id: u64,
    abort: tokio::task::AbortHandle,
    mutation_finished: Arc<AtomicBool>,
    completion: watch::Receiver<Option<OwnerRestoreResult>>,
}

impl OwnerRestoreTask {
    fn is_pending(&self) -> bool {
        self.completion.borrow().is_none()
    }
}

#[derive(Default)]
struct RejoinWatchdogGeneration(AtomicU64);

impl RejoinWatchdogGeneration {
    fn advance(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }

    /// Claims an armed generation exactly once. A reconnect, successful
    /// establishment, runtime drop, or replacement arm advances the epoch and
    /// makes the stale deadline harmless.
    fn claim(&self, expected: u64) -> bool {
        self.0
            .compare_exchange(
                expected,
                expected.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

struct DelegatedRejoinWatchdog {
    generation: Arc<RejoinWatchdogGeneration>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl DelegatedRejoinWatchdog {
    fn new() -> Self {
        Self {
            generation: Arc::new(RejoinWatchdogGeneration::default()),
            task: None,
        }
    }

    fn arm(
        &mut self,
        authority: Arc<AuthorityCell>,
        stamp: AuthorityStamp,
        delegation_generation: u64,
        coordinator: Option<DaemonDelegationCoordinator>,
    ) {
        // Advance before aborting so even a task already waking at the
        // deadline loses its compare-exchange against this newer rejoin.
        let watchdog_generation = self.generation.advance();
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let generation = Arc::clone(&self.generation);
        self.task = Some(tokio::spawn(async move {
            tokio::time::sleep(DELEGATED_REJOIN_SESSION_TIMEOUT).await;
            if !generation.claim(watchdog_generation)
                || stamp.origin()
                    != (AuthorityOrigin::Delegated {
                        generation: delegation_generation,
                    })
                || !authority.is_current(stamp)
            {
                return;
            }
            request_restore(coordinator.as_ref(), delegation_generation).await;
        }));
    }

    fn cancel(&mut self) {
        // Invalidate first: abort is cooperative and may race a waking task.
        self.generation.advance();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for DelegatedRejoinWatchdog {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone)]
struct OwnerSnapshot {
    queue: QueueAuthoritySnapshot,
    volume: f32,
    playback: OwnerPlaybackSnapshot,
}

#[derive(Clone, Copy)]
struct OwnerPlaybackSnapshot {
    track_id: u64,
    position_secs: u64,
    was_playing: bool,
    had_loaded_audio: bool,
}

struct OwnerActionFence {
    authority: Arc<AuthorityCell>,
}

impl OwnerActionFence {
    fn acquire(authority: Arc<AuthorityCell>) -> Self {
        authority.suspend_owner_actions();
        Self { authority }
    }
}

impl Drop for OwnerActionFence {
    fn drop(&mut self) {
        self.authority.resume_owner_actions();
    }
}

struct PreparedCommon {
    app: Arc<DaemonQconnectApp>,
    sink: Arc<DaemonEventSink>,
    config: WsTransportConfig,
    sync_state: Arc<AsyncMutex<QconnectRemoteSyncState>>,
    receiver: broadcast::Receiver<TransportEvent>,
    buffered: VecDeque<TransportEvent>,
    buffered_bytes: usize,
    stamp: AuthorityStamp,
    expected_current: AuthorityStamp,
    volume_mode: VolumeMode,
    custom_name: Option<String>,
    /// Captured from owner SESSION_STATE during preparation. `prepare_owner`
    /// cannot return until the matching SessionEstablished signal also arrives,
    /// and this value stays private until `commit_owner` installs that runtime.
    confirmed_owner_session_id: Option<Zeroizing<String>>,
}

pub struct PreparedDelegation {
    common: PreparedCommon,
    session_id: Zeroizing<String>,
    become_active: bool,
    owner_snapshot: Option<OwnerSnapshot>,
    transition_guard: Option<OwnedMutexGuard<()>>,
    transition_fence: Option<OwnerActionFence>,
}

pub struct PreparedOwner {
    common: PreparedCommon,
    transition_guard: OwnedMutexGuard<()>,
    transition_fence: OwnerActionFence,
}

enum PendingActivationKind {
    Delegated,
    Owner {
        volume: f32,
        playback: OwnerPlaybackSnapshot,
    },
}

/// Cancellation-safe release of a freshly installed runtime. Normal retirement
/// stops the old transport first; if the coordinator's cleanup timeout drops
/// that future, `Drop` still stops stale audio and releases the current loop.
struct PendingActivation {
    start: Option<oneshot::Sender<()>>,
    runtime: Runtime,
    authority: Arc<AuthorityCell>,
    stamp: AuthorityStamp,
    kind: PendingActivationKind,
    transition_guard: Option<OwnedMutexGuard<()>>,
    transition_fence: Option<OwnerActionFence>,
    owner_restore_task: Arc<StdMutex<Option<OwnerRestoreTask>>>,
    owner_quality: Quality,
}

/// The final release edge of an installed authority transition.
///
/// Owner rollback moves this guard into its tracked playback-restoration task,
/// keeping both ordinary owner actions and the next authority transition out
/// until the restored stream is stable. `Drop` is deliberately sufficient: a
/// coordinator cleanup timeout or task abort still wakes the installed event
/// loop and releases both gates.
struct DeferredActivationRelease {
    start: Option<oneshot::Sender<()>>,
    transition_fence: Option<OwnerActionFence>,
    transition_guard: Option<OwnedMutexGuard<()>>,
}

impl DeferredActivationRelease {
    fn new(
        start: Option<oneshot::Sender<()>>,
        transition_fence: Option<OwnerActionFence>,
        transition_guard: Option<OwnedMutexGuard<()>>,
    ) -> Self {
        Self {
            start,
            transition_fence,
            transition_guard,
        }
    }

    fn release(&mut self) {
        // Admission opens before the prepared loop wakes so buffered commands
        // cannot observe the transition fence and disappear as rejected work.
        self.transition_fence.take();
        if let Some(start) = self.start.take() {
            let _ = start.send(());
        }
        self.transition_guard.take();
    }
}

impl Drop for DeferredActivationRelease {
    fn drop(&mut self) {
        self.release();
    }
}

impl PendingActivation {
    fn release(&mut self) {
        let Some(start) = self.start.take() else {
            return;
        };
        if !self.authority.is_current(self.stamp) {
            self.transition_fence.take();
            self.transition_guard.take();
            return;
        }
        let _ = self.runtime.core().stop();
        if let PendingActivationKind::Owner { volume, .. } = &self.kind {
            if let Err(error) = self.runtime.core().set_volume(*volume) {
                log::warn!("[QConnect] failed to restore owner volume: {error}");
            }
        }
        if let PendingActivationKind::Owner { playback, .. } = &self.kind {
            let release = DeferredActivationRelease::new(
                Some(start),
                self.transition_fence.take(),
                self.transition_guard.take(),
            );
            schedule_owner_playback_restore(
                Arc::clone(&self.runtime),
                Arc::clone(&self.authority),
                self.stamp,
                *playback,
                self.owner_quality,
                Arc::clone(&self.owner_restore_task),
                release,
            );
        } else {
            let mut release = DeferredActivationRelease::new(
                Some(start),
                self.transition_fence.take(),
                self.transition_guard.take(),
            );
            release.release();
        }
    }
}

impl Drop for PendingActivation {
    fn drop(&mut self) {
        self.release();
    }
}

pub struct RetiredAuthority {
    runtime: Option<DaemonQconnectRuntime>,
    activation: Option<PendingActivation>,
}

/// Runtime-owned host. It contains no delegated credential material; prepared
/// candidates own and scrub that material until a successful stamped commit.
pub struct DaemonDelegationHost {
    runtime: Runtime,
    inner: Arc<StdMutex<DaemonQconnectInner>>,
    shared: SharedState,
    settings_db: std::path::PathBuf,
    custom_device_name: Arc<tokio::sync::RwLock<Option<String>>>,
    quality_cap: Arc<StdMutex<Quality>>,
    authority: Arc<AuthorityCell>,
    coordinator: OnceLock<DaemonDelegationCoordinator>,
    transition_gate: Arc<AsyncMutex<()>>,
    owner_snapshot: StdMutex<Option<OwnerSnapshot>>,
    owner_restore_task: Arc<StdMutex<Option<OwnerRestoreTask>>>,
    projection: DaemonLanProjectionSlot,
    shutdown_mode: AtomicU8,
}

impl DaemonDelegationHost {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime: Runtime,
        inner: Arc<StdMutex<DaemonQconnectInner>>,
        shared: SharedState,
        settings_db: std::path::PathBuf,
        custom_device_name: Arc<tokio::sync::RwLock<Option<String>>>,
        quality_cap: Arc<StdMutex<Quality>>,
        authority: Arc<AuthorityCell>,
    ) -> Self {
        Self {
            runtime,
            inner,
            shared,
            settings_db,
            custom_device_name,
            quality_cap,
            authority,
            coordinator: OnceLock::new(),
            transition_gate: Arc::new(AsyncMutex::new(())),
            owner_snapshot: StdMutex::new(None),
            owner_restore_task: Arc::new(StdMutex::new(None)),
            projection: DaemonLanProjectionSlot::default(),
            shutdown_mode: AtomicU8::new(SHUTDOWN_RESTORE_OWNER),
        }
    }

    pub fn install_coordinator(&self, coordinator: DaemonDelegationCoordinator) -> bool {
        self.coordinator.set(coordinator).is_ok()
    }

    pub fn attach_projection(&self, projection: LanProjection) {
        self.projection.attach(&self.authority, projection);
    }

    pub fn detach_projection(&self) {
        self.projection.detach();
    }

    pub fn projection_slot(&self) -> DaemonLanProjectionSlot {
        self.projection.clone()
    }

    pub fn current_projected_session_id(&self, stamp: AuthorityStamp) -> Option<String> {
        self.projection.current_session_id(&self.authority, stamp)
    }

    pub fn set_shutdown_restore_owner(&self, restore: bool) {
        self.shutdown_mode.store(
            if restore {
                SHUTDOWN_RESTORE_OWNER
            } else {
                SHUTDOWN_DISCARD_OWNER
            },
            Ordering::Release,
        );
    }

    fn coordinator(&self) -> Option<DaemonDelegationCoordinator> {
        self.coordinator.get().cloned()
    }

    pub async fn await_owner_playback_restore(&self) -> Result<(), &'static str> {
        // Clone the observation handles but leave the task in the host slot.
        // If this future is itself cancelled, a later caller can still await
        // the same restoration instead of silently detaching its JoinHandle.
        let task = recover_lock(&self.owner_restore_task).as_ref().cloned();
        let Some(mut task) = task else {
            return Ok(());
        };
        match tokio::time::timeout(
            Duration::from_secs(31),
            wait_for_owner_restore_completion(&mut task),
        )
        .await
        {
            Ok(result) => {
                clear_owner_restore_task_if(&self.owner_restore_task, task.id);
                result
            }
            Err(_) => {
                // The restoration has its own 30-second deadline, so this is
                // a stuck task/scheduler boundary. Abort the actual worker and
                // wait for its supervisor to join it. A pathological delayed
                // join remains in the slot for a later teardown retry.
                task.abort.abort();
                if tokio::time::timeout(
                    Duration::from_secs(1),
                    wait_for_owner_restore_completion(&mut task),
                )
                .await
                .is_ok()
                {
                    clear_owner_restore_task_if(&self.owner_restore_task, task.id);
                }
                Err("owner-playback-restore-timed-out")
            }
        }
    }

    pub fn owner_restore_pending(&self) -> bool {
        self.owner_snapshot_pending()
            || recover_lock(&self.owner_restore_task)
                .as_ref()
                .is_some_and(OwnerRestoreTask::is_pending)
    }

    pub fn owner_snapshot_pending(&self) -> bool {
        recover_lock(&self.owner_snapshot).is_some()
    }

    fn volume_mode(&self) -> VolumeMode {
        VolumeMode::from_kv(super::transport::load_volume_mode_at(&self.settings_db).as_deref())
    }

    async fn owner_client(&self) -> Result<QobuzClient, DelegationErrorCode> {
        self.runtime
            .core()
            .client()
            .read()
            .await
            .clone()
            .ok_or(DelegationErrorCode::ApiRejected)
    }

    fn make_app(
        &self,
        engine: DaemonRendererEngine,
        stamp: AuthorityStamp,
    ) -> (
        Arc<DaemonQconnectApp>,
        Arc<DaemonEventSink>,
        Arc<AsyncMutex<QconnectRemoteSyncState>>,
    ) {
        let transport = Arc::new(NativeWsTransport::new());
        let sync_state = Arc::new(AsyncMutex::new(QconnectRemoteSyncState::default()));
        let sink = Arc::new(DaemonEventSink::new(
            engine,
            Arc::clone(&sync_state),
            Arc::clone(&self.authority),
            stamp,
            self.projection.clone(),
        ));
        let app = Arc::new(QconnectApp::new(
            transport,
            Arc::clone(&sink),
            Arc::clone(&sync_state),
        ));
        sink.set_app(&app);
        (app, sink, sync_state)
    }

    async fn disconnect_common(common: PreparedCommon) {
        let _ = common.app.disconnect().await;
    }

    async fn stop_runtime(&self, runtime: DaemonQconnectRuntime) {
        runtime.event_loop.abort();
        {
            let mut sync = runtime.sync_state.lock().await;
            sync.watchdog_generation = sync.watchdog_generation.wrapping_add(1);
            sync.session = QconnectSessionState::default();
            sync.session_renderer_states.clear();
        }
        let _ = runtime.app.disconnect().await;
        let _ = runtime.event_loop.await;
    }

    async fn restore_owner_snapshot(&self) -> Option<OwnerPlaybackSnapshot> {
        // Clone first and clear only after the atomic replacement completes.
        // If shutdown cancellation drops this future, the original snapshot
        // remains available to the idempotent teardown retry.
        let snapshot = recover_lock(&self.owner_snapshot).as_ref().cloned();
        let Some(snapshot) = snapshot else {
            return None;
        };
        let _ = self.runtime.core().stop();
        self.runtime
            .core()
            .restore_authority_snapshot(snapshot.queue)
            .await;
        let _ = self.runtime.core().set_volume(snapshot.volume);
        let state = self.runtime.core().get_queue_state().await;
        let bus = self
            .shared
            .lock()
            .ok()
            .and_then(|shared| shared.bus.clone());
        if let Some(bus) = bus {
            let _ = bus.send(CoreEvent::QueueUpdated { state });
        }
        recover_lock(&self.owner_snapshot).take();
        Some(snapshot.playback)
    }

    fn publish_origin(&self, origin: AuthorityOrigin) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.qconnect.credential_origin = match origin {
                AuthorityOrigin::Owner => "owner",
                AuthorityOrigin::Delegated { .. } => "delegated",
            }
            .to_string();
            shared.emit_qconnect_session_changed();
        }
    }
}

#[async_trait]
impl DelegationHost for DaemonDelegationHost {
    type Candidate = HandoffCandidate;
    type PreparedDelegation = PreparedDelegation;
    type PreparedOwner = PreparedOwner;
    type RetiredAuthority = RetiredAuthority;

    async fn prepare_delegation(
        &self,
        generation: u64,
        candidate: HandoffCandidate,
        cancellation: DelegationCancellation,
    ) -> Result<PreparedDelegation, DelegationErrorCode> {
        let expected_current = self
            .authority
            .current()
            .ok_or(DelegationErrorCode::CommitRejected)?;
        let app_credentials = self
            .owner_client()
            .await?
            .delegated_app_credentials()
            .await
            .map_err(|_| DelegationErrorCode::ApiRejected)?;

        let (session_id, api_token, qws_token, become_active) = candidate.into_parts();
        let session_id = Zeroizing::new(session_id);
        let (api_endpoint, api_exp, api_jwt) = api_token.into_parts();
        let api_endpoint = Zeroizing::new(api_endpoint);
        let mut api_jwt = Zeroizing::new(api_jwt);
        let (qws_endpoint, _qws_exp, qws_jwt) = qws_token.into_parts();
        let mut qws_endpoint = Zeroizing::new(qws_endpoint);
        let mut qws_jwt = Zeroizing::new(qws_jwt);
        let api_exp = u64::try_from(api_exp).map_err(|_| DelegationErrorCode::CandidateExpired)?;
        let api_host = reqwest::Url::parse(&api_endpoint)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .ok_or(DelegationErrorCode::ApiRejected)?;
        let endpoint = DelegatedApiEndpoint::new(&api_endpoint, &[api_host.as_str()])
            .map_err(|_| DelegationErrorCode::ApiRejected)?;
        let api_config = DelegatedApiConfig::new(
            endpoint,
            api_exp,
            std::mem::take(&mut *api_jwt),
            app_credentials,
        )
        .map_err(|_| DelegationErrorCode::ApiRejected)?;
        let delegated_client = Arc::new(
            DelegatedQobuzClient::new(api_config).map_err(|_| DelegationErrorCode::ApiRejected)?,
        );

        let stamp = self
            .authority
            .reserve(AuthorityOrigin::Delegated { generation });
        let volume_mode = self.volume_mode();
        let engine = DaemonRendererEngine::delegated(
            Arc::clone(&self.runtime),
            Arc::clone(&delegated_client),
            volume_mode,
            Arc::clone(&self.quality_cap),
            Arc::clone(&self.authority),
            stamp,
        );
        let (app, sink, sync_state) = self.make_app(engine, stamp);
        let receiver = app.subscribe_transport_events();
        let mut config = WsTransportConfig::default();
        config.endpoint_url = std::mem::take(&mut *qws_endpoint);
        config.jwt_qws = Some(std::mem::take(&mut *qws_jwt));
        config.require_jwt = true;
        config.subscribe_channels = vec![vec![0x01], vec![0x02], vec![0x03]];
        config.reconnect_idle_retry_ms = 60_000;
        app.connect(config.clone())
            .await
            .map_err(|_| DelegationErrorCode::QwsRejected)?;

        let custom_name = self.custom_device_name.read().await.clone();
        let mut common = PreparedCommon {
            app,
            sink,
            config,
            sync_state,
            receiver,
            buffered: VecDeque::new(),
            buffered_bytes: 0,
            stamp,
            expected_current,
            volume_mode,
            custom_name,
            confirmed_owner_session_id: None,
        };
        let api_validation = async {
            delegated_client
                .validate_access()
                .await
                .map_err(|_| DelegationErrorCode::ApiRejected)
        };
        tokio::try_join!(
            api_validation,
            wait_for_qws_ready(&mut common, cancellation)
        )?;

        Ok(PreparedDelegation {
            common,
            session_id,
            become_active,
            owner_snapshot: None,
            transition_guard: None,
            transition_fence: None,
        })
    }

    async fn activate_delegation(
        &self,
        _generation: u64,
        prepared: &mut PreparedDelegation,
        cancellation: DelegationCancellation,
    ) -> Result<(), DelegationErrorCode> {
        if !self.authority.is_current(prepared.common.expected_current) {
            return Err(DelegationErrorCode::CandidateCancelled);
        }
        // Serialize the fallible activation/commit/retirement tail before
        // closing ordinary action admission. The guard remains in `prepared`
        // through commit and moves into `PendingActivation`; owner rollback
        // keeps it until playback is restored. A second fence is not a mutex,
        // so this explicit gate is what prevents overlapping transitions.
        prepared.transition_guard = Some(Arc::clone(&self.transition_gate).lock_owned().await);

        // Fence every installed authority, including delegated -> delegated:
        // runtime action permits can span catalog/stream awaits and must drain
        // before cloud activation. Join is deliberately the final I/O before
        // the stamped swap; once SET_ACTIVE=true arrives, commit stays short.
        // Only the owner additionally needs a queue snapshot for restore.
        prepared.transition_fence = Some(OwnerActionFence::acquire(Arc::clone(&self.authority)));
        self.authority.wait_for_actions_drained().await;
        if !self.authority.is_current(prepared.common.expected_current) {
            return Err(DelegationErrorCode::CandidateCancelled);
        }
        if prepared.common.expected_current.origin() == AuthorityOrigin::Owner {
            let queue = self.runtime.core().capture_authority_snapshot().await;
            if !self.authority.is_current(prepared.common.expected_current) {
                return Err(DelegationErrorCode::CandidateCancelled);
            }
            let playback_state = self.runtime.core().get_playback_state();
            let queue_track_id = self
                .runtime
                .core()
                .current_track()
                .await
                .map(|track| track.id);
            if !self.authority.is_current(prepared.common.expected_current) {
                return Err(DelegationErrorCode::CandidateCancelled);
            }
            prepared.owner_snapshot = Some(OwnerSnapshot {
                queue,
                volume: playback_state.volume,
                playback: OwnerPlaybackSnapshot {
                    track_id: playback_state.track_id,
                    position_secs: playback_state.position,
                    was_playing: playback_state.is_playing,
                    had_loaded_audio: self.runtime.core().player().has_loaded_audio()
                        && playback_state.track_id != 0
                        && queue_track_id == Some(playback_state.track_id),
                },
            });
        }

        send_delegated_join(
            &prepared.common.app,
            prepared.session_id.as_str(),
            prepared.become_active,
            prepared.common.custom_name.as_deref(),
            current_quality(&self.quality_cap),
        )
        .await?;
        wait_for_activation(&mut prepared.common, cancellation).await?;
        if !self.authority.is_current(prepared.common.expected_current) {
            return Err(DelegationErrorCode::CandidateCancelled);
        }
        Ok(())
    }

    fn commit_delegation(
        &self,
        generation: u64,
        prepared: PreparedDelegation,
    ) -> Result<RetiredAuthority, CommitRejected<PreparedDelegation>> {
        let mut inner = lock_inner(&self.inner);
        if prepared.common.stamp.origin() != (AuthorityOrigin::Delegated { generation })
            || !self.authority.is_current(prepared.common.expected_current)
            || inner.runtime.as_ref().map(|runtime| runtime.stamp)
                != Some(prepared.common.expected_current)
            || prepared.transition_guard.is_none()
            || prepared.transition_fence.is_none()
        {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        }
        if prepared.common.expected_current.origin() == AuthorityOrigin::Owner
            && prepared.owner_snapshot.is_none()
        {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        }
        if !self.projection.install_authority(
            &self.authority,
            prepared.common.stamp,
            Some(prepared.session_id.as_str()),
        ) {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        }

        let retired = inner.runtime.take().expect("runtime stamp was checked");
        retired.event_loop.abort();
        let PreparedDelegation {
            common,
            session_id,
            become_active: _,
            owner_snapshot,
            transition_guard,
            transition_fence,
        } = prepared;
        let (runtime, start) = spawn_delegated_runtime(
            common,
            session_id,
            generation,
            Arc::clone(&self.inner),
            Arc::clone(&self.shared),
            Arc::clone(&self.authority),
            self.coordinator(),
            current_quality(&self.quality_cap),
        );
        inner.last_error = None;
        inner.last_pushed_queue_ids = None;
        let installed_stamp = runtime.stamp;
        inner.runtime = Some(runtime);
        drop(inner);

        if let Some(snapshot) = owner_snapshot {
            *recover_lock(&self.owner_snapshot) = Some(snapshot);
        }
        self.publish_origin(AuthorityOrigin::Delegated { generation });
        Ok(RetiredAuthority {
            runtime: Some(retired),
            activation: Some(PendingActivation {
                start: Some(start),
                runtime: Arc::clone(&self.runtime),
                authority: Arc::clone(&self.authority),
                stamp: installed_stamp,
                kind: PendingActivationKind::Delegated,
                transition_guard,
                transition_fence,
                owner_restore_task: Arc::clone(&self.owner_restore_task),
                owner_quality: current_quality(&self.quality_cap),
            }),
        })
    }

    async fn discard_delegation(&self, _generation: u64, prepared: PreparedDelegation) {
        Self::disconnect_common(prepared.common).await;
    }

    async fn prepare_owner(
        &self,
        _generation: u64,
        _reason: RestoreReason,
        cancellation: DelegationCancellation,
    ) -> Result<PreparedOwner, DelegationErrorCode> {
        let expected_current = self
            .authority
            .current()
            .filter(|stamp| matches!(stamp.origin(), AuthorityOrigin::Delegated { .. }))
            .ok_or(DelegationErrorCode::CommitRejected)?;
        let config = resolve_transport_config(&self.runtime)
            .await
            .map_err(|_| DelegationErrorCode::OwnerRestoreFailed)?;
        let stamp = self.authority.reserve(AuthorityOrigin::Owner);
        let volume_mode = self.volume_mode();
        let engine = DaemonRendererEngine::owner(
            Arc::clone(&self.runtime),
            volume_mode,
            Arc::clone(&self.quality_cap),
            Arc::clone(&self.authority),
            stamp,
        );
        let (app, sink, sync_state) = self.make_app(engine, stamp);
        let receiver = app.subscribe_transport_events();
        app.connect(config.clone())
            .await
            .map_err(|_| DelegationErrorCode::OwnerRestoreFailed)?;
        let custom_name = self.custom_device_name.read().await.clone();
        let mut common = PreparedCommon {
            app,
            sink,
            config,
            sync_state,
            receiver,
            buffered: VecDeque::new(),
            buffered_bytes: 0,
            stamp,
            expected_current,
            volume_mode,
            custom_name,
            confirmed_owner_session_id: None,
        };
        wait_for_qws_ready(&mut common, cancellation.clone())
            .await
            .map_err(|_| DelegationErrorCode::OwnerRestoreFailed)?;
        bootstrap_prepared_owner_presence(&common.app, common.custom_name.clone())
            .await
            .map_err(|_| DelegationErrorCode::OwnerRestoreFailed)?;
        wait_for_owner_session(&mut common, cancellation)
            .await
            .map_err(|_| DelegationErrorCode::OwnerRestoreFailed)?;
        let transition_guard = Arc::clone(&self.transition_gate).lock_owned().await;
        let transition_fence = OwnerActionFence::acquire(Arc::clone(&self.authority));
        self.authority.wait_for_actions_drained().await;
        if !self.authority.is_current(expected_current) {
            return Err(DelegationErrorCode::CandidateCancelled);
        }
        Ok(PreparedOwner {
            common,
            transition_guard,
            transition_fence,
        })
    }

    fn commit_owner(
        &self,
        _generation: u64,
        prepared: PreparedOwner,
    ) -> Result<RetiredAuthority, CommitRejected<PreparedOwner>> {
        let mut inner = lock_inner(&self.inner);
        if !self.authority.is_current(prepared.common.expected_current)
            || inner.runtime.as_ref().map(|runtime| runtime.stamp)
                != Some(prepared.common.expected_current)
        {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        }
        let stamp = prepared.common.stamp;
        let Some(owner_session_id) = prepared.common.confirmed_owner_session_id.clone() else {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        };
        let Some(owner_snapshot) = recover_lock(&self.owner_snapshot).take() else {
            return Err(CommitRejected::new(
                prepared,
                DelegationErrorCode::CommitRejected,
            ));
        };
        let OwnerSnapshot {
            queue,
            volume,
            playback,
        } = owner_snapshot;
        let queue_state = match self
            .runtime
            .core()
            .try_commit_authority_snapshot(queue, || {
                self.projection.install_authority(
                    &self.authority,
                    stamp,
                    Some(owner_session_id.as_str()),
                )
            }) {
            Ok(state) => state,
            Err(queue) => {
                *recover_lock(&self.owner_snapshot) = Some(OwnerSnapshot {
                    queue,
                    volume,
                    playback,
                });
                return Err(CommitRejected::new(
                    prepared,
                    DelegationErrorCode::CommitRejected,
                ));
            }
        };
        let retired = inner.runtime.take().expect("runtime stamp was checked");
        retired.event_loop.abort();
        let PreparedOwner {
            common,
            transition_guard,
            transition_fence,
        } = prepared;
        let (runtime, start) = spawn_owner_runtime(
            common,
            Arc::clone(&self.inner),
            Arc::clone(&self.runtime),
            Arc::clone(&self.shared),
            Arc::clone(&self.authority),
            self.projection.clone(),
        );
        inner.last_error = None;
        inner.last_pushed_queue_ids = None;
        inner.runtime = Some(runtime);
        drop(inner);

        let bus = self
            .shared
            .lock()
            .ok()
            .and_then(|shared| shared.bus.clone());
        if let Some(bus) = bus {
            let _ = bus.send(CoreEvent::QueueUpdated { state: queue_state });
        }

        self.publish_origin(AuthorityOrigin::Owner);
        Ok(RetiredAuthority {
            runtime: Some(retired),
            activation: Some(PendingActivation {
                start: Some(start),
                runtime: Arc::clone(&self.runtime),
                authority: Arc::clone(&self.authority),
                stamp,
                kind: PendingActivationKind::Owner { volume, playback },
                transition_guard: Some(transition_guard),
                transition_fence: Some(transition_fence),
                owner_restore_task: Arc::clone(&self.owner_restore_task),
                owner_quality: current_quality(&self.quality_cap),
            }),
        })
    }

    async fn discard_owner(&self, _generation: u64, prepared: PreparedOwner) {
        Self::disconnect_common(prepared.common).await;
    }

    async fn retire_authority(&self, mut retired: RetiredAuthority) {
        if let Some(runtime) = retired.runtime.take() {
            self.stop_runtime(runtime).await;
        }

        if let Some(mut activation) = retired.activation.take() {
            activation.release();
        }
    }

    async fn shutdown_authority(&self) {
        // A restore from the preceding authority transition owns this gate
        // until it has either stabilized playback or failed. Waiting here is
        // intentional: cancelling that valid restore would lose the original
        // owner stream just because a later candidate/shutdown arrived.
        let transition_guard = Arc::clone(&self.transition_gate).lock_owned().await;
        let shutdown_fence = OwnerActionFence::acquire(Arc::clone(&self.authority));
        self.authority.wait_for_actions_drained().await;
        let previous = self.projection.clear_authority(&self.authority);
        let was_delegated = previous
            .is_some_and(|stamp| matches!(stamp.origin(), AuthorityOrigin::Delegated { .. }));
        let runtime = {
            let mut inner = lock_inner(&self.inner);
            inner.lifecycle_state = QconnectLifecycleState::Off;
            inner.last_pushed_queue_ids = None;
            inner.runtime.take()
        };
        if let Some(runtime) = runtime {
            runtime.event_loop.abort();
            self.stop_runtime(runtime).await;
        }
        let has_owner_snapshot = recover_lock(&self.owner_snapshot).is_some();
        let playback_restore = if (was_delegated || has_owner_snapshot)
            && self.shutdown_mode.load(Ordering::Acquire) == SHUTDOWN_RESTORE_OWNER
        {
            self.restore_owner_snapshot().await
        } else if was_delegated || has_owner_snapshot {
            let _ = self.runtime.core().stop();
            recover_lock(&self.owner_snapshot).take();
            None
        } else {
            // A pending candidate may have captured a snapshot, but disabling an
            // installed owner must not stop or replace local playback.
            recover_lock(&self.owner_snapshot).take();
            None
        };
        self.publish_origin(AuthorityOrigin::Owner);
        discard_completed_owner_restore_task(&self.owner_restore_task);
        let mut release =
            DeferredActivationRelease::new(None, Some(shutdown_fence), Some(transition_guard));
        if let Some(playback) = playback_restore {
            schedule_offline_owner_playback_restore(
                Arc::clone(&self.runtime),
                Arc::clone(&self.authority),
                playback,
                current_quality(&self.quality_cap),
                Arc::clone(&self.owner_restore_task),
                release,
            );
        } else {
            release.release();
        }
    }
}

fn recover_lock<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn next_owner_restore_task_id() -> u64 {
    NEXT_OWNER_RESTORE_TASK_ID
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .expect("owner playback restore task id space exhausted")
}

async fn wait_for_owner_restore_completion(task: &mut OwnerRestoreTask) -> OwnerRestoreResult {
    loop {
        if let Some(result) = *task.completion.borrow_and_update() {
            return result;
        }
        if task.completion.changed().await.is_err() {
            return (*task.completion.borrow())
                .unwrap_or(Err("owner-playback-restore-task-failed"));
        }
    }
}

fn clear_owner_restore_task_if(slot: &StdMutex<Option<OwnerRestoreTask>>, expected_id: u64) {
    let mut task = recover_lock(slot);
    if task.as_ref().map(|task| task.id) == Some(expected_id) {
        task.take();
    }
}

fn discard_completed_owner_restore_task(slot: &StdMutex<Option<OwnerRestoreTask>>) {
    let mut task = recover_lock(slot);
    if task
        .as_ref()
        .is_some_and(|task| task.mutation_finished.load(Ordering::Acquire))
    {
        task.take();
    }
}

/// Installs a restoration before its start barrier is opened. A live task can
/// only own this slot while it also owns `transition_gate`, so seeing one here
/// is an invariant failure. Preserve that valid older task and cancel only the
/// impossible overlapping newcomer.
fn install_owner_restore_task(
    slot: &StdMutex<Option<OwnerRestoreTask>>,
    task: OwnerRestoreTask,
) -> bool {
    let mut current = recover_lock(slot);
    if current
        .as_ref()
        .is_some_and(|current| !current.mutation_finished.load(Ordering::Acquire))
    {
        log::error!("[QConnect] overlapping owner playback restoration rejected");
        task.abort.abort();
        return false;
    }
    *current = Some(task);
    true
}

/// Spawn the mutating worker behind a start barrier and a non-cancellable
/// supervisor. The supervisor owns the transition release, joins the worker,
/// then publishes completion. Callers can therefore abandon their wait without
/// detaching either the mutation or its authority gates.
fn spawn_tracked_owner_restore<F>(
    restore: F,
    mut release: DeferredActivationRelease,
) -> (OwnerRestoreTask, oneshot::Sender<()>)
where
    F: Future<Output = OwnerRestoreResult> + Send + 'static,
{
    let id = next_owner_restore_task_id();
    let (start, started) = oneshot::channel();
    let worker = tokio::spawn(async move {
        if started.await.is_err() {
            return Err("owner-playback-restore-cancelled");
        }
        restore.await
    });
    let abort = worker.abort_handle();
    let mutation_finished = Arc::new(AtomicBool::new(false));
    let supervisor_finished = Arc::clone(&mutation_finished);
    let (completion_tx, completion) = watch::channel(None);
    tokio::spawn(async move {
        let result = match worker.await {
            Ok(result) => result,
            Err(error) if error.is_cancelled() => Err("owner-playback-restore-cancelled"),
            Err(_) => Err("owner-playback-restore-task-failed"),
        };

        // Mark the mutation finished before opening the transition mutex. A
        // newly admitted transition may run on another executor thread before
        // the watch publication below, but it can now replace this task slot
        // without aborting or losing a live restoration.
        supervisor_finished.store(true, Ordering::Release);
        release.release();
        completion_tx.send_replace(Some(result));
    });
    (
        OwnerRestoreTask {
            id,
            abort,
            mutation_finished,
            completion,
        },
        start,
    )
}

fn schedule_owner_playback_restore(
    runtime: Runtime,
    authority: Arc<AuthorityCell>,
    stamp: AuthorityStamp,
    playback: OwnerPlaybackSnapshot,
    quality: Quality,
    slot: Arc<StdMutex<Option<OwnerRestoreTask>>>,
    mut release: DeferredActivationRelease,
) {
    discard_completed_owner_restore_task(&slot);
    if !playback.had_loaded_audio {
        release.release();
        return;
    }
    let restore = async move {
        if !authority.is_current(stamp) {
            return Err("owner-playback-restore-authority-changed");
        }
        let restore = async {
            let current = runtime.core().current_track().await;
            if current.as_ref().map(|track| track.id) != Some(playback.track_id)
                || !authority.is_current(stamp)
            {
                return Err("restored owner queue does not match playback snapshot".to_string());
            }
            runtime
                .core()
                .play_track_resolved(
                    playback.track_id,
                    quality,
                    None,
                    None,
                    playback.position_secs,
                )
                .await?;
            if !authority.is_current(stamp) {
                return Err("owner playback authority changed".to_string());
            }
            if !playback.was_playing {
                runtime.core().pause().map_err(|error| error.to_string())?;
            }
            Ok::<(), String>(())
        };
        match tokio::time::timeout(Duration::from_secs(30), restore).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => {
                if authority.is_current(stamp) {
                    let _ = runtime.core().stop();
                }
                log::warn!("[QConnect] owner playback restoration failed");
                Err("owner-playback-restore-failed")
            }
            Err(_) => {
                if authority.is_current(stamp) {
                    let _ = runtime.core().stop();
                }
                log::warn!("[QConnect] owner playback restoration timed out");
                Err("owner-playback-restore-timed-out")
            }
        }
    };
    let (task, start) = spawn_tracked_owner_restore(restore, release);
    if install_owner_restore_task(&slot, task) {
        let _ = start.send(());
    }
}

fn schedule_offline_owner_playback_restore(
    runtime: Runtime,
    authority: Arc<AuthorityCell>,
    playback: OwnerPlaybackSnapshot,
    quality: Quality,
    slot: Arc<StdMutex<Option<OwnerRestoreTask>>>,
    mut release: DeferredActivationRelease,
) {
    discard_completed_owner_restore_task(&slot);
    if !playback.had_loaded_audio {
        release.release();
        return;
    }
    let restore = async move {
        if authority.current().is_some() {
            return Err("owner-playback-restore-authority-changed");
        }
        let restore = async {
            let current = runtime.core().current_track().await;
            if current.as_ref().map(|track| track.id) != Some(playback.track_id)
                || authority.current().is_some()
            {
                return Err("restored owner queue does not match playback snapshot".to_string());
            }
            runtime
                .core()
                .play_track_resolved(
                    playback.track_id,
                    quality,
                    None,
                    None,
                    playback.position_secs,
                )
                .await?;
            if authority.current().is_some() {
                return Err("owner-playback-restore-authority-changed".to_string());
            }
            if !playback.was_playing {
                runtime.core().pause().map_err(|error| error.to_string())?;
            }
            Ok::<(), String>(())
        };
        match tokio::time::timeout(Duration::from_secs(30), restore).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => {
                if authority.current().is_none() {
                    let _ = runtime.core().stop();
                }
                log::warn!("[QConnect] offline owner playback restoration failed");
                Err("owner-playback-restore-failed")
            }
            Err(_) => {
                if authority.current().is_none() {
                    let _ = runtime.core().stop();
                }
                log::warn!("[QConnect] offline owner playback restoration timed out");
                Err("owner-playback-restore-timed-out")
            }
        }
    };
    let (task, start) = spawn_tracked_owner_restore(restore, release);
    if install_owner_restore_task(&slot, task) {
        let _ = start.send(());
    }
}

fn current_quality(quality: &StdMutex<Quality>) -> Quality {
    *recover_lock(quality)
}

fn quality_wire(quality: Quality) -> i32 {
    match quality {
        Quality::Mp3 => 1,
        Quality::Lossless => 2,
        Quality::HiRes => 3,
        Quality::UltraHiRes => 4,
    }
}

fn retained_event_bytes(event: &TransportEvent) -> usize {
    match event {
        TransportEvent::InboundPayloadBytes { payload, .. } => payload.len(),
        _ => 0,
    }
}

fn should_retain(event: &TransportEvent) -> bool {
    matches!(
        event,
        TransportEvent::Connected
            | TransportEvent::Disconnected
            | TransportEvent::SessionEstablished
            | TransportEvent::InboundPayloadBytes { .. }
            | TransportEvent::InboundQueueServerEvent(_)
            | TransportEvent::InboundRendererServerCommand(_)
            | TransportEvent::InboundReceived(_)
    )
}

fn retain_event(
    common: &mut PreparedCommon,
    event: TransportEvent,
) -> Result<(), DelegationErrorCode> {
    if !should_retain(&event) {
        return Ok(());
    }
    let bytes = retained_event_bytes(&event);
    if common.buffered.len() >= PREFLIGHT_BUFFER_EVENTS
        || common.buffered_bytes.saturating_add(bytes) > PREFLIGHT_BUFFER_BYTES
    {
        return Err(DelegationErrorCode::Internal);
    }
    common.buffered_bytes = common.buffered_bytes.saturating_add(bytes);
    common.buffered.push_back(event);
    Ok(())
}

async fn next_preflight_event(
    common: &mut PreparedCommon,
    mut cancellation: DelegationCancellation,
) -> Result<TransportEvent, DelegationErrorCode> {
    tokio::select! {
        _ = cancellation.cancelled() => Err(DelegationErrorCode::CandidateCancelled),
        event = common.receiver.recv() => match event {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Lagged(_)) => Err(DelegationErrorCode::Internal),
            Err(broadcast::error::RecvError::Closed) => Err(DelegationErrorCode::QwsRejected),
        }
    }
}

async fn wait_for_qws_ready(
    common: &mut PreparedCommon,
    cancellation: DelegationCancellation,
) -> Result<(), DelegationErrorCode> {
    let mut authenticated = false;
    let mut subscribed = false;
    while !(authenticated && subscribed) {
        let event = next_preflight_event(common, cancellation.clone()).await?;
        match &event {
            TransportEvent::Authenticated => authenticated = true,
            TransportEvent::Subscribed => subscribed = true,
            TransportEvent::Disconnected
            | TransportEvent::CloudError { .. }
            | TransportEvent::MaxReconnectAttemptsExceeded { .. } => {
                return Err(DelegationErrorCode::QwsRejected)
            }
            _ => {}
        }
        retain_event(common, event)?;
    }
    Ok(())
}

async fn wait_for_activation(
    common: &mut PreparedCommon,
    cancellation: DelegationCancellation,
) -> Result<(), DelegationErrorCode> {
    loop {
        let event = next_preflight_event(common, cancellation.clone()).await?;
        let accepted = matches!(
            &event,
            TransportEvent::InboundRendererServerCommand(command)
                if command.command_type == RendererCommandType::SrvrRndrSetActive
                    && command.payload.get("active").and_then(serde_json::Value::as_bool)
                        == Some(true)
        );
        if matches!(
            &event,
            TransportEvent::Disconnected
                | TransportEvent::CloudError { .. }
                | TransportEvent::MaxReconnectAttemptsExceeded { .. }
        ) {
            return Err(DelegationErrorCode::ActivationRejected);
        }
        retain_event(common, event)?;
        if accepted {
            return Ok(());
        }
    }
}

/// Owner preparation is complete only after the cloud has accepted the
/// controller-side join and emitted the same establishment signal consumed by
/// a live session loop. Authentication/subscription and a successful socket
/// send are necessary but not sufficient to publish `OwnerReady`.
async fn wait_for_owner_session(
    common: &mut PreparedCommon,
    cancellation: DelegationCancellation,
) -> Result<(), DelegationErrorCode> {
    let mut established = false;
    loop {
        let event = next_preflight_event(common, cancellation.clone()).await?;
        if let Some(session_id) = owner_session_id_from_event(&event).map(str::to_string) {
            common.confirmed_owner_session_id = Some(Zeroizing::new(session_id));
        }
        established |= matches!(&event, TransportEvent::SessionEstablished);
        if matches!(
            &event,
            TransportEvent::Disconnected
                | TransportEvent::CloudError { .. }
                | TransportEvent::MaxReconnectAttemptsExceeded { .. }
        ) {
            return Err(DelegationErrorCode::OwnerRestoreFailed);
        }
        retain_event(common, event)?;
        if established && common.confirmed_owner_session_id.is_some() {
            return Ok(());
        }
    }
}

fn owner_session_id_from_event(event: &TransportEvent) -> Option<&str> {
    match event {
        TransportEvent::InboundQueueServerEvent(event)
            if event.message_type() == "MESSAGE_TYPE_SRVR_CTRL_SESSION_STATE" =>
        {
            event
                .payload
                .get("session_uuid")
                .and_then(serde_json::Value::as_str)
                .filter(|session_id| !session_id.is_empty())
        }
        _ => None,
    }
}

async fn send_delegated_join(
    app: &Arc<DaemonQconnectApp>,
    session_id: &str,
    become_active: bool,
    custom_name: Option<&str>,
    quality: Quality,
) -> Result<(), DelegationErrorCode> {
    let mut device_info = default_qconnect_device_info_with_name(custom_name);
    if let Some(capabilities) = device_info.capabilities.as_mut() {
        capabilities.max_audio_quality = Some(quality_wire(quality));
    }
    let queue_version = app.queue_state_snapshot().await.version;
    let report = RendererReport::new(
        RendererReportType::RndrSrvrJoinSession,
        Uuid::new_v4().to_string(),
        queue_version,
        serde_json::json!({
            "session_uuid": session_id,
            "device_info": serde_json::to_value(device_info).unwrap_or_default(),
            "is_active": become_active,
            "reason": JOIN_SESSION_REASON_CONTROLLER_REQUEST,
            "initial_state": {
                "playing_state": PLAYING_STATE_STOPPED,
                "buffer_state": RendererBufferState::Ok.as_i32(),
                "current_position": 0,
                "duration": 0,
                "queue_version": {
                    "major": queue_version.major,
                    "minor": queue_version.minor,
                }
            }
        }),
    );
    app.send_renderer_report_command(report)
        .await
        .map_err(|_| DelegationErrorCode::ActivationRejected)
}

#[allow(clippy::too_many_arguments)]
fn spawn_delegated_runtime(
    common: PreparedCommon,
    session_id: Zeroizing<String>,
    generation: u64,
    inner: Arc<StdMutex<DaemonQconnectInner>>,
    shared: SharedState,
    authority: Arc<AuthorityCell>,
    coordinator: Option<DaemonDelegationCoordinator>,
    quality: Quality,
) -> (DaemonQconnectRuntime, oneshot::Sender<()>) {
    let PreparedCommon {
        app,
        sink,
        config,
        sync_state,
        receiver,
        buffered,
        stamp,
        custom_name,
        ..
    } = common;
    let (start, started) = oneshot::channel();
    let loop_app = Arc::clone(&app);
    let loop_sink = Arc::clone(&sink);
    let loop_sync = Arc::clone(&sync_state);
    let loop_authority = Arc::clone(&authority);
    let event_loop = tokio::spawn(async move {
        if started.await.is_err() || !loop_authority.is_current(stamp) {
            return;
        }
        loop_sink
            .on_event(QconnectAppEvent::TransportConnected)
            .await;
        run_delegated_loop(
            loop_app,
            loop_sink,
            loop_sync,
            receiver,
            buffered,
            session_id,
            generation,
            inner,
            shared,
            loop_authority,
            stamp,
            coordinator,
            custom_name,
            quality,
        )
        .await;
    });
    (
        DaemonQconnectRuntime {
            stamp,
            app,
            config,
            event_loop,
            sync_state,
        },
        start,
    )
}

fn spawn_owner_runtime(
    common: PreparedCommon,
    inner: Arc<StdMutex<DaemonQconnectInner>>,
    runtime: Runtime,
    shared: SharedState,
    authority: Arc<AuthorityCell>,
    projection: DaemonLanProjectionSlot,
) -> (DaemonQconnectRuntime, oneshot::Sender<()>) {
    let PreparedCommon {
        app,
        sink,
        config,
        sync_state,
        receiver,
        buffered,
        stamp,
        volume_mode,
        ..
    } = common;
    let idle_retry_active = config.reconnect_idle_retry_ms > 0;
    let host: Arc<dyn SessionLoopHost> = Arc::new(DaemonSessionLoopHost {
        app: Arc::clone(&app),
        sync_state: Arc::clone(&sync_state),
        inner,
        authority: Arc::clone(&authority),
        stamp,
        sink,
        runtime,
        shared,
        volume_mode,
        projection,
    });
    let (start, started) = oneshot::channel();
    let loop_app = Arc::clone(&app);
    let loop_authority = Arc::clone(&authority);
    let event_loop = tokio::spawn(async move {
        if started.await.is_err() || !loop_authority.is_current(stamp) {
            return;
        }
        loop_app
            .run_session_loop_with_prefetched(host, receiver, idle_retry_active, buffered)
            .await;
    });
    (
        DaemonQconnectRuntime {
            stamp,
            app,
            config,
            event_loop,
            sync_state,
        },
        start,
    )
}

#[allow(clippy::too_many_arguments)]
async fn run_delegated_loop(
    app: Arc<DaemonQconnectApp>,
    sink: Arc<DaemonEventSink>,
    _sync_state: Arc<AsyncMutex<QconnectRemoteSyncState>>,
    mut receiver: broadcast::Receiver<TransportEvent>,
    buffered: VecDeque<TransportEvent>,
    session_id: Zeroizing<String>,
    generation: u64,
    inner: Arc<StdMutex<DaemonQconnectInner>>,
    shared: SharedState,
    authority: Arc<AuthorityCell>,
    stamp: AuthorityStamp,
    coordinator: Option<DaemonDelegationCoordinator>,
    custom_name: Option<String>,
    quality: Quality,
) {
    let mut disconnected = false;
    let mut rejoin_watchdog = DelegatedRejoinWatchdog::new();
    for event in buffered {
        if !handle_delegated_event(
            &app,
            &sink,
            event,
            &session_id,
            generation,
            &inner,
            &shared,
            &authority,
            stamp,
            coordinator.as_ref(),
            custom_name.as_deref(),
            quality,
            &mut disconnected,
            &mut rejoin_watchdog,
        )
        .await
        {
            return;
        }
    }
    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                request_restore(coordinator.as_ref(), generation).await;
                return;
            }
            Err(broadcast::error::RecvError::Closed) => {
                request_restore(coordinator.as_ref(), generation).await;
                return;
            }
        };
        if !handle_delegated_event(
            &app,
            &sink,
            event,
            &session_id,
            generation,
            &inner,
            &shared,
            &authority,
            stamp,
            coordinator.as_ref(),
            custom_name.as_deref(),
            quality,
            &mut disconnected,
            &mut rejoin_watchdog,
        )
        .await
        {
            return;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_delegated_event(
    app: &Arc<DaemonQconnectApp>,
    sink: &Arc<DaemonEventSink>,
    event: TransportEvent,
    session_id: &str,
    generation: u64,
    inner: &Arc<StdMutex<DaemonQconnectInner>>,
    shared: &SharedState,
    authority: &Arc<AuthorityCell>,
    stamp: AuthorityStamp,
    coordinator: Option<&DaemonDelegationCoordinator>,
    custom_name: Option<&str>,
    quality: Quality,
    disconnected: &mut bool,
    rejoin_watchdog: &mut DelegatedRejoinWatchdog,
) -> bool {
    if !authority.is_current(stamp) {
        return false;
    }
    match &event {
        TransportEvent::Disconnected => {
            *disconnected = true;
            rejoin_watchdog.cancel();
            update_lifecycle_state_if_running(
                inner,
                sink,
                shared,
                authority,
                stamp,
                QconnectLifecycleState::Reconnecting,
            )
            .await;
        }
        TransportEvent::Subscribed if *disconnected => {
            if send_delegated_join(app, session_id, true, custom_name, quality)
                .await
                .is_err()
            {
                request_restore(coordinator, generation).await;
                return false;
            }
            rejoin_watchdog.arm(
                Arc::clone(authority),
                stamp,
                generation,
                coordinator.cloned(),
            );
        }
        TransportEvent::SessionEstablished => {
            *disconnected = false;
            rejoin_watchdog.cancel();
            update_lifecycle_state_if_running(
                inner,
                sink,
                shared,
                authority,
                stamp,
                QconnectLifecycleState::Connected,
            )
            .await;
        }
        TransportEvent::CloudError { .. } | TransportEvent::MaxReconnectAttemptsExceeded { .. } => {
            request_restore(coordinator, generation).await;
            return false;
        }
        _ => {}
    }
    if !authority.is_current(stamp) {
        return false;
    }
    if let Err(error) = app.handle_transport_event(event).await {
        if authority.is_current(stamp) {
            log::warn!("[QConnect] delegated event application failed: {error}");
            request_restore(coordinator, generation).await;
        }
        return false;
    }
    authority.is_current(stamp)
}

async fn request_restore(coordinator: Option<&DaemonDelegationCoordinator>, generation: u64) {
    if let Some(coordinator) = coordinator {
        let _ = coordinator
            .restore_owner_if_active(generation, RestoreReason::TransportFatal)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_mapping_never_exceeds_the_configured_cap() {
        assert_eq!(quality_wire(Quality::Mp3), 1);
        assert_eq!(quality_wire(Quality::Lossless), 2);
        assert_eq!(quality_wire(Quality::HiRes), 3);
        assert_eq!(quality_wire(Quality::UltraHiRes), 4);
    }

    #[test]
    fn preflight_retains_only_events_needed_after_commit() {
        assert!(!should_retain(&TransportEvent::Authenticated));
        assert!(!should_retain(&TransportEvent::KeepalivePingSent));
        assert!(should_retain(&TransportEvent::Disconnected));
        assert!(should_retain(&TransportEvent::SessionEstablished));
    }

    #[test]
    fn rejoin_watchdog_generation_is_claimed_once() {
        let generation = RejoinWatchdogGeneration::default();
        let armed = generation.advance();

        assert!(generation.claim(armed));
        assert!(!generation.claim(armed));
    }

    #[test]
    fn rejoin_watchdog_ignores_cancelled_and_replaced_generations() {
        let generation = RejoinWatchdogGeneration::default();
        let cancelled = generation.advance();
        generation.advance();
        let replacement = generation.advance();

        assert!(!generation.claim(cancelled));
        assert!(generation.claim(replacement));
    }
}
