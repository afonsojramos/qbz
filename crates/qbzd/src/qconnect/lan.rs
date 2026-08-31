//! Thin qbzd adapter for the official QConnect LAN receiver surface.
//!
//! This module projects the daemon's existing QConnect identity into the LAN
//! wire, owns the listener/mDNS lifetime, and drains the bounded latest-wins
//! admission slot on one dedicated thread. Cloud validation and transactional
//! authority switching deliberately remain outside this adapter.

use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::JoinHandle;
use std::time::Duration;

use qbz_models::Quality;
use qconnect_lan::{
    ConnectInfo, DeviceType, DisplayInfo, EndpointPolicy, HandoffCandidate, LanError,
    LanProjection, LanService, LanServiceConfig, MaxAudioQuality,
};

use super::authority::{AuthorityCell, AuthorityOrigin, AuthorityStamp};
use super::transport::default_qconnect_device_info;

const ADMISSION_WAIT: Duration = Duration::from_secs(60);
const DAEMON_BRAND: &str = "QBZ";
const DAEMON_MODEL: &str = "QBZ Daemon";
const ADMISSION_THREAD_NAME: &str = "qbzd-qconnect-lan-admission";

pub type HandoffCallback = Arc<dyn Fn(HandoffCandidate) + Send + Sync + 'static>;

/// Stamp-aware bridge between runtime authority and the synchronous LAN
/// projection read by the HTTP worker.
///
/// Every qbzd authority install/clear is serialized through this short-held
/// mutex. An old runtime event can therefore never overwrite the session of a
/// replacement runtime, and a prepared candidate is not exposed until the same
/// critical section that installs its authority.
#[derive(Clone, Default)]
pub(crate) struct DaemonLanProjectionSlot {
    inner: Arc<StdMutex<ProjectionSlotState>>,
}

#[derive(Default)]
struct ProjectionSlotState {
    projection: Option<LanProjection>,
    installed: Option<InstalledSessionProjection>,
}

struct InstalledSessionProjection {
    stamp: AuthorityStamp,
    session_id: Option<String>,
}

impl DaemonLanProjectionSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, ProjectionSlotState> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            log::error!("[QConnect LAN] projection slot recovered from a poisoned lock");
            let guard = poisoned.into_inner();
            self.inner.clear_poison();
            guard
        })
    }

    fn write_projection(state: &ProjectionSlotState, session_id: Option<String>) {
        if let Some(projection) = state.projection.as_ref() {
            projection.set_current_session_id(session_id);
        }
    }

    /// Install a runtime and its already-confirmed public session atomically
    /// with respect to every projection update. `session_id` is `None` for a
    /// fresh owner until its first cloud SESSION_STATE arrives.
    pub fn install_authority(
        &self,
        authority: &AuthorityCell,
        stamp: AuthorityStamp,
        session_id: Option<&str>,
    ) -> bool {
        let mut state = self.lock();

        // Fail closed during the tiny install boundary: the old session may no
        // longer be current, while exposing the candidate before install would
        // violate the handoff transaction.
        Self::write_projection(&state, None);
        if !authority.install(stamp) {
            let current = authority.current();
            let restore = state
                .installed
                .as_ref()
                .filter(|installed| Some(installed.stamp) == current)
                .and_then(|installed| installed.session_id.clone());
            if current != state.installed.as_ref().map(|installed| installed.stamp) {
                state.installed = None;
            }
            Self::write_projection(&state, restore);
            return false;
        }

        let session_id = session_id.map(str::to_string);
        state.installed = Some(InstalledSessionProjection {
            stamp,
            session_id: session_id.clone(),
        });
        Self::write_projection(&state, session_id);
        true
    }

    /// Publish a cloud-confirmed owner session from an event belonging to the
    /// exact installed runtime. Delegated session ids enter only through
    /// `install_authority` after their transactional commit.
    pub fn confirm_owner_session(
        &self,
        authority: &AuthorityCell,
        stamp: AuthorityStamp,
        session_id: &str,
    ) -> bool {
        if stamp.origin() != AuthorityOrigin::Owner || session_id.is_empty() {
            return false;
        }

        let mut state = self.lock();
        if !authority.is_current(stamp)
            || state.installed.as_ref().map(|installed| installed.stamp) != Some(stamp)
        {
            return false;
        }

        let session_id = session_id.to_string();
        if let Some(installed) = state.installed.as_mut() {
            installed.session_id = Some(session_id.clone());
        }
        Self::write_projection(&state, Some(session_id));
        true
    }

    /// Snapshot the exact installed session for listener construction. This
    /// seeds GET connect-info before the HTTP worker can accept its first call.
    pub fn current_session_id(
        &self,
        authority: &AuthorityCell,
        expected: AuthorityStamp,
    ) -> Option<String> {
        let state = self.lock();
        if !authority.is_current(expected) {
            return None;
        }
        state
            .installed
            .as_ref()
            .filter(|installed| installed.stamp == expected)
            .and_then(|installed| installed.session_id.clone())
    }

    pub fn attach(&self, authority: &AuthorityCell, projection: LanProjection) {
        let mut state = self.lock();
        let current = authority.current();
        let session_id = state
            .installed
            .as_ref()
            .filter(|installed| Some(installed.stamp) == current)
            .and_then(|installed| installed.session_id.clone());
        projection.set_current_session_id(session_id);
        state.projection = Some(projection);
    }

    pub fn detach(&self) {
        self.lock().projection.take();
    }

    pub fn clear_authority(&self, authority: &AuthorityCell) -> Option<AuthorityStamp> {
        let mut state = self.lock();
        Self::write_projection(&state, None);
        let previous = authority.clear();
        state.installed = None;
        previous
    }

    pub fn clear_if_current(&self, authority: &AuthorityCell, expected: AuthorityStamp) -> bool {
        let mut state = self.lock();
        if !authority.is_current(expected) {
            return false;
        }

        Self::write_projection(&state, None);
        if authority.clear_if_current(expected) {
            state.installed = None;
            true
        } else {
            let restore = state
                .installed
                .as_ref()
                .filter(|installed| authority.is_current(installed.stamp))
                .and_then(|installed| installed.session_id.clone());
            Self::write_projection(&state, restore);
            false
        }
    }
}

#[derive(Debug)]
pub enum DaemonLanError {
    ExistingIdentityUnavailable,
    Service(LanError),
    AdmissionThread(std::io::Error),
}

impl fmt::Display for DaemonLanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExistingIdentityUnavailable => {
                formatter.write_str("qconnect LAN identity is unavailable")
            }
            Self::Service(error) => write!(formatter, "qconnect LAN service failed: {error}"),
            Self::AdmissionThread(_) => {
                formatter.write_str("qconnect LAN admission thread could not start")
            }
        }
    }
}

impl Error for DaemonLanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Service(error) => Some(error),
            Self::AdmissionThread(error) => Some(error),
            Self::ExistingIdentityUnavailable => None,
        }
    }
}

impl From<LanError> for DaemonLanError {
    fn from(error: LanError) -> Self {
        Self::Service(error)
    }
}

/// Map the daemon's effective Qobuz quality ceiling to the exact official LAN
/// enum. `UltraHiRes` is QBZ's 24-bit/>96 kHz tier and tops out at the Qobuz
/// 192 kHz format family; QBZ does not advertise the unused 384 kHz value.
pub const fn max_audio_quality_for_quality(quality: Quality) -> MaxAudioQuality {
    match quality {
        Quality::Mp3 => MaxAudioQuality::MP3,
        Quality::Lossless => MaxAudioQuality::UpToCd,
        Quality::HiRes => MaxAudioQuality::UpToHires96,
        Quality::UltraHiRes => MaxAudioQuality::UpToHires192,
    }
}

struct ExistingIdentity {
    device_uuid: String,
    friendly_name: String,
    software_version: String,
}

impl ExistingIdentity {
    fn resolve() -> Result<Self, DaemonLanError> {
        let device_info = default_qconnect_device_info();
        let (Some(device_uuid), Some(friendly_name), Some(software_version)) = (
            device_info.device_uuid,
            device_info.friendly_name,
            device_info.software_version,
        ) else {
            return Err(DaemonLanError::ExistingIdentityUnavailable);
        };
        if device_uuid.trim().is_empty()
            || friendly_name.trim().is_empty()
            || software_version.trim().is_empty()
        {
            return Err(DaemonLanError::ExistingIdentityUnavailable);
        }
        Ok(Self {
            device_uuid,
            friendly_name,
            software_version,
        })
    }
}

fn build_projection(
    identity: &ExistingIdentity,
    app_id: String,
    quality: Quality,
    current_session_id: Option<String>,
) -> LanProjection {
    LanProjection::new(
        DisplayInfo {
            friendly_name: identity.friendly_name.clone(),
            serial_number: identity.device_uuid.clone(),
            brand_display_name: DAEMON_BRAND.to_string(),
            model_display_name: DAEMON_MODEL.to_string(),
            max_audio_quality: max_audio_quality_for_quality(quality),
            device_type: DeviceType::Streamer,
            software_version: identity.software_version.clone(),
        },
        ConnectInfo {
            app_id,
            current_session_id,
        },
    )
}

/// Owns the LAN service and its blocking admission bridge.
///
/// The callback must enqueue or otherwise hand off the candidate promptly; it
/// runs serially on the dedicated bridge thread so callback invocation order is
/// the same order observed from the latest-wins admission slot.
pub struct DaemonLanRuntime {
    service: Option<LanService>,
    projection: LanProjection,
    admission_bridge: Option<JoinHandle<()>>,
}

impl DaemonLanRuntime {
    pub fn start(
        endpoint_policy: EndpointPolicy,
        app_id: String,
        quality: Quality,
        current_session_id: Option<String>,
        on_handoff: HandoffCallback,
    ) -> Result<Self, DaemonLanError> {
        let identity = ExistingIdentity::resolve()?;
        let projection = build_projection(&identity, app_id, quality, current_session_id);
        let config =
            LanServiceConfig::new(projection.clone(), endpoint_policy, identity.device_uuid);
        let (mut service, inbox) = LanService::start(config)?;

        let admission_bridge = match std::thread::Builder::new()
            .name(ADMISSION_THREAD_NAME.to_string())
            .spawn(move || loop {
                match inbox.take_timeout(ADMISSION_WAIT) {
                    Some(candidate) => on_handoff(candidate),
                    None if inbox.is_closed() => break,
                    None => {}
                }
            }) {
            Ok(thread) => thread,
            Err(error) => {
                service.shutdown();
                return Err(DaemonLanError::AdmissionThread(error));
            }
        };

        Ok(Self {
            service: Some(service),
            projection,
            admission_bridge: Some(admission_bridge),
        })
    }

    pub fn projection(&self) -> LanProjection {
        self.projection.clone()
    }

    pub fn port(&self) -> Option<u16> {
        self.service.as_ref().map(LanService::port)
    }

    /// Idempotent teardown in the normative order: withdraw discovery and stop
    /// HTTP admission first, then wait for the now-woken bridge to exit.
    pub fn shutdown(&mut self) {
        if let Some(mut service) = self.service.take() {
            service.shutdown();
        }
        if let Some(bridge) = self.admission_bridge.take() {
            let _ = bridge.join();
        }
    }
}

impl Drop for DaemonLanRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_identity() -> ExistingIdentity {
        ExistingIdentity {
            device_uuid: "de0c0d70-0000-4000-8000-000000000001".to_string(),
            friendly_name: "Living room".to_string(),
            software_version: "qbz/2.1.0".to_string(),
        }
    }

    #[test]
    fn quality_mapping_matches_official_lan_families() {
        assert_eq!(
            max_audio_quality_for_quality(Quality::Mp3),
            MaxAudioQuality::MP3
        );
        assert_eq!(
            max_audio_quality_for_quality(Quality::Lossless),
            MaxAudioQuality::UpToCd
        );
        assert_eq!(
            max_audio_quality_for_quality(Quality::HiRes),
            MaxAudioQuality::UpToHires96
        );
        assert_eq!(
            max_audio_quality_for_quality(Quality::UltraHiRes),
            MaxAudioQuality::UpToHires192
        );
    }

    #[test]
    fn daemon_projection_uses_existing_identity_and_official_role() {
        let projection = build_projection(
            &fixture_identity(),
            "app-id".to_string(),
            Quality::UltraHiRes,
            None,
        );

        assert_eq!(
            projection.display_info(),
            DisplayInfo {
                friendly_name: "Living room".to_string(),
                serial_number: "de0c0d70-0000-4000-8000-000000000001".to_string(),
                brand_display_name: "QBZ".to_string(),
                model_display_name: "QBZ Daemon".to_string(),
                max_audio_quality: MaxAudioQuality::UpToHires192,
                device_type: DeviceType::Streamer,
                software_version: "qbz/2.1.0".to_string(),
            }
        );
        assert_eq!(
            projection.connect_info(),
            ConnectInfo {
                app_id: "app-id".to_string(),
                current_session_id: None,
            }
        );
    }

    #[test]
    fn projection_session_id_changes_only_when_explicitly_updated() {
        let projection = build_projection(
            &fixture_identity(),
            "app-id".to_string(),
            Quality::Lossless,
            None,
        );
        assert_eq!(projection.connect_info().current_session_id, None);

        projection.set_current_session_id(Some("delegated-session".to_string()));
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("delegated-session")
        );
        projection.set_current_session_id(None);
        assert_eq!(projection.connect_info().current_session_id, None);
    }

    #[test]
    fn stamped_projection_publishes_only_installed_authority() {
        let authority = AuthorityCell::new();
        let slot = DaemonLanProjectionSlot::default();
        let projection = build_projection(
            &fixture_identity(),
            "app-id".to_string(),
            Quality::Lossless,
            None,
        );
        slot.attach(&authority, projection.clone());

        let owner = authority.reserve(AuthorityOrigin::Owner);
        assert!(slot.install_authority(&authority, owner, None));
        assert!(slot.confirm_owner_session(&authority, owner, "owner-session"));
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("owner-session")
        );

        // Merely reserving/preparing a guest cannot expose its candidate id.
        let guest = authority.reserve(AuthorityOrigin::Delegated { generation: 7 });
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("owner-session")
        );

        assert!(slot.install_authority(&authority, guest, Some("guest-session")));
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("guest-session")
        );

        // A late event from the retired owner cannot overwrite the guest.
        assert!(!slot.confirm_owner_session(&authority, owner, "stale-owner-session"));
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("guest-session")
        );
    }

    #[test]
    fn owner_session_events_update_and_stale_clear_cannot_win() {
        let authority = AuthorityCell::new();
        let slot = DaemonLanProjectionSlot::default();
        let projection = build_projection(
            &fixture_identity(),
            "app-id".to_string(),
            Quality::Lossless,
            None,
        );
        slot.attach(&authority, projection.clone());

        let first_owner = authority.reserve(AuthorityOrigin::Owner);
        assert!(slot.install_authority(&authority, first_owner, None));
        assert!(slot.confirm_owner_session(&authority, first_owner, "owner-session-a"));
        assert!(slot.confirm_owner_session(&authority, first_owner, "owner-session-b"));
        assert_eq!(
            slot.current_session_id(&authority, first_owner).as_deref(),
            Some("owner-session-b")
        );

        let next_owner = authority.reserve(AuthorityOrigin::Owner);
        assert!(slot.install_authority(&authority, next_owner, Some("owner-session-c")));
        assert!(!slot.clear_if_current(&authority, first_owner));
        assert_eq!(
            projection.connect_info().current_session_id.as_deref(),
            Some("owner-session-c")
        );
    }
}
