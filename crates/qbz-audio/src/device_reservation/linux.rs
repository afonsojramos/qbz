//! Linux implementation of `DeviceReservation`.
//!
//! Implements a client of the `org.freedesktop.ReserveDevice1` D-Bus protocol
//! (specified by the PulseAudio project, also implemented by PipeWire,
//! WirePlumber, JACK, MPD, Roon Bridge, etc.). For an ALSA device string of
//! the form `hw:N,M` (or `plughw:`, or `hw:CARD=Name`), we map the *card*
//! index `N` to the well-known bus name `org.freedesktop.ReserveDevice1.AudioN`
//! and request ownership of it.
//!
//! Acquisition algorithm (matches the spec at
//! `qbz-nix-docs/specs/2026-05-07-alsa-exclusive-hardening-design.md`):
//!
//!   1. `RequestName` with `DO_NOT_QUEUE`.
//!   2. `PrimaryOwner` / `AlreadyOwner`  -> we own it. Done.
//!   3. `Exists` / `InQueue`             -> someone else holds it. Read their
//!                                          `Priority` property; if our
//!                                          priority is higher, call
//!                                          `RequestRelease(our_priority)`
//!                                          on the holder. If that returns
//!                                          `true`, retry `RequestName` with
//!                                          `DO_NOT_QUEUE | REPLACE_EXISTING`.
//!   4. Anything else (zbus error, refusal, equal-or-lower priority) -> err.
//!
//! On `Drop`, an active guard releases the bus name. A *degraded* guard
//! (returned when the session bus is unavailable) is a no-op on `Drop`.

use std::fmt;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, Proxy};
use zbus::fdo::{ReleaseNameReply, RequestNameFlags, RequestNameReply};
use zbus::names::WellKnownName;

/// Priority QBZ takes when acquiring a `ReserveDevice1` reservation.
///
/// Rationale (from the design spec): PulseAudio and PipeWire run at `0`, pro
/// audio software (Ardour, Bitwig, Roon Bridge) runs at `10`-`30`. We pick `5`:
/// above the system mixer so we can pre-empt PipeWire when the user toggles
/// exclusive mode, well below pro DAW software so we never stomp on a
/// recording session.
pub(crate) const QBZ_PRIORITY: i32 = 5;

/// Application name advertised over D-Bus when we publish the
/// `ReserveDevice1` interface as a server. Currently captured in logs only;
/// publishing the server side is deferred (see Task 2 spec note).
#[allow(dead_code)]
pub(crate) const QBZ_APPLICATION_NAME: &str = "QBZ";

/// D-Bus interface every `ReserveDevice1` holder publishes under
/// `/org/freedesktop/ReserveDevice1/AudioN`.
const RESERVE_DEVICE1_INTERFACE: &str = "org.freedesktop.ReserveDevice1";

#[derive(Debug)]
pub struct DeviceReservation {
    state: ReservationState,
}

#[derive(Debug)]
enum ReservationState {
    /// We own the bus name `bus_name` on `connection`. `Drop` releases it.
    /// `app_device_name` is stashed for Task 5 (status payload) — kept private.
    Active {
        connection: Connection,
        bus_name: String,
        #[allow(dead_code)] // Surfaced via Tauri status command in Task 5.
        app_device_name: String,
    },
    /// D-Bus session bus was unreachable, or some other graceful-degrade
    /// path. `is_active()` reports `false`; `Drop` is a no-op.
    Degraded,
}

impl DeviceReservation {
    /// Acquire a D-Bus device reservation for the given ALSA `hw:` device.
    ///
    /// Returns:
    ///   - `Ok(active_guard)` once we own the bus name.
    ///   - `Ok(degraded_guard)` if the session bus is unreachable. The caller
    ///     should treat playback as "no reservation, but proceed normally".
    ///   - `Err(InvalidDevice)` for unparseable device strings.
    ///   - `Err(HigherPriorityHolder { .. })` if another app refuses to
    ///     release, or holds at equal-or-greater priority.
    ///   - `Err(DbusError(_))` for protocol-level zbus failures we can't
    ///     downgrade (e.g. malformed bus name, reply marshaling failure).
    ///   - `Err(AlsaError(_))` for ALSA enumeration failures while resolving
    ///     symbolic card names like `hw:CARD=DacMagic`.
    pub fn acquire(hw_device: &str, app_device_name: &str) -> Result<Self, ReservationError> {
        let card = parse_card_index(hw_device)?;
        let bus_name = bus_name_for_card(card);
        let object_path = object_path_for_card(card);

        // Connect to the session bus. Failure here is *not* an error from the
        // caller's perspective — we degrade and let playback proceed.
        let connection = match Connection::session() {
            Ok(c) => c,
            Err(e) => {
                log::warn!(
                    "[reservation] D-Bus session bus unavailable, degrading: {}",
                    e
                );
                return Ok(Self {
                    state: ReservationState::Degraded,
                });
            }
        };

        match try_acquire_name(&connection, &bus_name, false)? {
            // Either we just took ownership, or we already owned this name on
            // this same connection (idempotent for Lifetime-A nested under
            // Lifetime-B in Task 5).
            RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
                log::debug!("[reservation] acquired {}", bus_name);
                Ok(Self {
                    state: ReservationState::Active {
                        connection,
                        bus_name,
                        app_device_name: app_device_name.to_string(),
                    },
                })
            }
            // Someone else holds it (or is queued). Check their priority and
            // ask them to step aside.
            RequestNameReply::Exists | RequestNameReply::InQueue => {
                resolve_contention(&connection, &bus_name, &object_path, app_device_name)
            }
        }
    }

    /// Whether this guard currently holds an active D-Bus reservation.
    pub fn is_active(&self) -> bool {
        matches!(self.state, ReservationState::Active { .. })
    }
}

impl Drop for DeviceReservation {
    fn drop(&mut self) {
        if let ReservationState::Active {
            connection,
            bus_name,
            ..
        } = &self.state
        {
            match release_name(connection, bus_name) {
                Ok(ReleaseNameReply::Released) => {
                    log::debug!("[reservation] released {}", bus_name);
                }
                Ok(ReleaseNameReply::NonExistent) => {
                    // We thought we owned it but the bus daemon disagrees.
                    // Almost always indicates a logic bug in our state
                    // tracking — surface loudly.
                    log::warn!(
                        "[reservation] release_name returned NonExistent for {} \
                         (we believed we owned it)",
                        bus_name
                    );
                }
                Ok(ReleaseNameReply::NotOwner) => {
                    log::warn!(
                        "[reservation] release_name returned NotOwner for {} \
                         (we believed we owned it)",
                        bus_name
                    );
                }
                Err(e) => {
                    log::warn!("[reservation] release_name failed for {}: {}", bus_name, e);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ReservationError {
    InvalidDevice(String),
    HigherPriorityHolder {
        holder_name: String,
        holder_priority: i32,
    },
    DbusError(String),
    AlsaError(String),
}

impl fmt::Display for ReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDevice(s) => write!(f, "invalid ALSA device string: {}", s),
            Self::HigherPriorityHolder {
                holder_name,
                holder_priority,
            } => write!(
                f,
                "device reserved by '{}' at priority {}",
                holder_name, holder_priority
            ),
            Self::DbusError(s) => write!(f, "D-Bus error: {}", s),
            Self::AlsaError(s) => write!(f, "ALSA error: {}", s),
        }
    }
}

impl std::error::Error for ReservationError {}

/// Issue `RequestName` for `bus_name`. Always sets `DO_NOT_QUEUE`; sets
/// `REPLACE_EXISTING` when `replace` is true (used after a successful
/// `RequestRelease` on the previous holder).
///
/// Returns the `RequestNameReply` on success. zbus errors are surfaced as
/// `ReservationError::DbusError`.
fn try_acquire_name(
    conn: &Connection,
    bus_name: &str,
    replace: bool,
) -> Result<RequestNameReply, ReservationError> {
    let proxy = DBusProxy::new(conn).map_err(|e| {
        ReservationError::DbusError(format!("DBusProxy::new failed: {}", e))
    })?;
    let well_known: WellKnownName<'_> = bus_name.try_into().map_err(|e| {
        ReservationError::DbusError(format!("invalid bus name '{}': {}", bus_name, e))
    })?;
    let flags = if replace {
        RequestNameFlags::DoNotQueue | RequestNameFlags::ReplaceExisting
    } else {
        RequestNameFlags::DoNotQueue.into()
    };
    proxy
        .request_name(well_known, flags)
        .map_err(|e| ReservationError::DbusError(format!("request_name failed: {}", e)))
}

/// Release the bus name. Pure forward of the zbus reply variant; the caller
/// (`Drop`) decides what to log. Returns `zbus::fdo::Error` which already
/// implements `Display`.
fn release_name(conn: &Connection, bus_name: &str) -> zbus::fdo::Result<ReleaseNameReply> {
    let proxy = DBusProxy::new(conn)?;
    // `zbus::fdo::Error` impls `From<zbus::Error>`, so `?` handles the names
    // crate's TryFrom error via the zbus -> fdo conversion chain.
    let well_known: WellKnownName<'_> = bus_name.try_into().map_err(zbus::Error::from)?;
    proxy.release_name(well_known)
}

/// We tried to acquire a held bus name and got `Exists` (or `InQueue`).
/// Inspect the holder's `Priority`, decide whether to ask them to release,
/// and either retry or return a `HigherPriorityHolder` error.
fn resolve_contention(
    conn: &Connection,
    bus_name: &str,
    object_path: &str,
    app_device_name: &str,
) -> Result<DeviceReservation, ReservationError> {
    // Default to 0 if the holder is uncooperative or doesn't expose Priority.
    // Rationale: PulseAudio/PipeWire are the most common holders and run at
    // priority 0; treating an unreadable priority as 0 lets us still pre-empt
    // them. Pro apps that *do* publish at higher priority will refuse via
    // RequestRelease anyway, which we honour below.
    let holder_priority = read_holder_priority(conn, bus_name, object_path).unwrap_or(0);

    if QBZ_PRIORITY <= holder_priority {
        let holder_name = read_holder_app_name(conn, bus_name, object_path)
            .unwrap_or_else(|| "another application".to_string());
        log::info!(
            "[reservation] {} held by {} at priority {} (>= ours {}); refusing",
            bus_name,
            holder_name,
            holder_priority,
            QBZ_PRIORITY
        );
        return Err(ReservationError::HigherPriorityHolder {
            holder_name,
            holder_priority,
        });
    }

    log::debug!(
        "[reservation] {} held at priority {}; calling RequestRelease({})",
        bus_name,
        holder_priority,
        QBZ_PRIORITY
    );

    let released = request_release_from_holder(conn, bus_name, object_path, QBZ_PRIORITY)?;
    if !released {
        let holder_name = read_holder_app_name(conn, bus_name, object_path)
            .unwrap_or_else(|| "another application".to_string());
        log::info!(
            "[reservation] {} held by {} refused RequestRelease",
            bus_name,
            holder_name
        );
        return Err(ReservationError::HigherPriorityHolder {
            holder_name,
            holder_priority,
        });
    }

    // Holder agreed to release. Retry with REPLACE_EXISTING.
    match try_acquire_name(conn, bus_name, true)? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            log::debug!("[reservation] acquired {} after RequestRelease", bus_name);
            Ok(DeviceReservation {
                state: ReservationState::Active {
                    connection: conn.clone(),
                    bus_name: bus_name.to_string(),
                    app_device_name: app_device_name.to_string(),
                },
            })
        }
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            // Someone slipped in between the holder releasing and us
            // re-requesting. Rare; surfaces as a generic D-Bus error.
            Err(ReservationError::DbusError(format!(
                "lost race after holder released {}",
                bus_name
            )))
        }
    }
}

/// Read the holder's `Priority` property via `org.freedesktop.DBus.Properties.Get`.
/// Returns `None` if the holder is uncooperative (no such property, type
/// mismatch, etc.); the caller treats that as priority 0.
fn read_holder_priority(conn: &Connection, bus_name: &str, object_path: &str) -> Option<i32> {
    let proxy = Proxy::new(conn, bus_name, object_path, RESERVE_DEVICE1_INTERFACE).ok()?;
    proxy.get_property::<i32>("Priority").ok()
}

/// Read the holder's `ApplicationName` property. Used purely for human-readable
/// error messages in `HigherPriorityHolder`.
fn read_holder_app_name(conn: &Connection, bus_name: &str, object_path: &str) -> Option<String> {
    let proxy = Proxy::new(conn, bus_name, object_path, RESERVE_DEVICE1_INTERFACE).ok()?;
    proxy.get_property::<String>("ApplicationName").ok()
}

/// Call `RequestRelease(priority)` on the current holder. Returns the
/// holder's reply (`true` = will release, `false` = refuses).
fn request_release_from_holder(
    conn: &Connection,
    bus_name: &str,
    object_path: &str,
    priority: i32,
) -> Result<bool, ReservationError> {
    let proxy = Proxy::new(conn, bus_name, object_path, RESERVE_DEVICE1_INTERFACE)
        .map_err(|e| ReservationError::DbusError(format!("Proxy::new for holder failed: {}", e)))?;
    proxy
        .call::<_, _, bool>("RequestRelease", &(priority,))
        .map_err(|e| ReservationError::DbusError(format!("RequestRelease failed: {}", e)))
}

/// Parse an ALSA device string and return the kernel card index.
///
/// Accepts: `"hw:0"`, `"hw:0,0"`, `"hw:1,0"`, `"plughw:1,0"`,
/// `"hw:CARD=DacMagic"`, `"hw:CARD=DacMagic,DEV=0"`.
pub(crate) fn parse_card_index(hw_device: &str) -> Result<u32, ReservationError> {
    let trimmed = hw_device.trim();
    let after_prefix = trimmed
        .strip_prefix("hw:")
        .or_else(|| trimmed.strip_prefix("plughw:"))
        .ok_or_else(|| ReservationError::InvalidDevice(hw_device.to_string()))?;

    let card_part = after_prefix.split(',').next().unwrap_or("");
    let card_part = card_part.trim();

    if card_part.is_empty() {
        return Err(ReservationError::InvalidDevice(hw_device.to_string()));
    }

    if let Some(name) = card_part.strip_prefix("CARD=") {
        resolve_card_index_by_name(name)
    } else {
        card_part
            .parse::<u32>()
            .map_err(|_| ReservationError::InvalidDevice(hw_device.to_string()))
    }
}

/// Resolve a symbolic ALSA card name (e.g., `"DacMagic"`) to its kernel index
/// by iterating over `alsa::card::Iter`.
fn resolve_card_index_by_name(name: &str) -> Result<u32, ReservationError> {
    for card in alsa::card::Iter::new() {
        let card = card.map_err(|e| ReservationError::AlsaError(e.to_string()))?;
        let id = card.get_name().unwrap_or_default();
        if id == name {
            return Ok(card.get_index() as u32);
        }
    }
    Err(ReservationError::InvalidDevice(format!(
        "ALSA card '{}' not found",
        name
    )))
}

/// Format the well-known D-Bus bus name for a given ALSA card index.
pub(crate) fn bus_name_for_card(card_index: u32) -> String {
    format!("org.freedesktop.ReserveDevice1.Audio{}", card_index)
}

/// Format the D-Bus object path for a given ALSA card index.
pub(crate) fn object_path_for_card(card_index: u32) -> String {
    format!("/org/freedesktop/ReserveDevice1/Audio{}", card_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_card_index_basic() {
        assert_eq!(parse_card_index("hw:0").unwrap(), 0);
        assert_eq!(parse_card_index("hw:1,0").unwrap(), 1);
        assert_eq!(parse_card_index("plughw:2,0").unwrap(), 2);
        assert_eq!(parse_card_index("hw:99,3").unwrap(), 99);
    }

    #[test]
    fn parse_card_index_rejects_garbage() {
        assert!(matches!(
            parse_card_index("default"),
            Err(ReservationError::InvalidDevice(_))
        ));
        assert!(matches!(
            parse_card_index("hw:"),
            Err(ReservationError::InvalidDevice(_))
        ));
        assert!(matches!(
            parse_card_index(""),
            Err(ReservationError::InvalidDevice(_))
        ));
    }

    #[test]
    fn bus_name_format() {
        assert_eq!(
            bus_name_for_card(0),
            "org.freedesktop.ReserveDevice1.Audio0"
        );
        assert_eq!(
            bus_name_for_card(7),
            "org.freedesktop.ReserveDevice1.Audio7"
        );
        assert_eq!(
            bus_name_for_card(99),
            "org.freedesktop.ReserveDevice1.Audio99"
        );
    }

    #[test]
    fn object_path_format() {
        assert_eq!(
            object_path_for_card(0),
            "/org/freedesktop/ReserveDevice1/Audio0"
        );
    }

    #[test]
    fn degraded_guard_reports_inactive() {
        // Construct a degraded guard directly. We cannot rely on
        // `acquire("hw:0,0", "test")` here because once Task 2 wires the
        // real D-Bus client, that call may succeed (returning an *active*
        // guard) on a developer machine running PipeWire.
        let g = DeviceReservation {
            state: ReservationState::Degraded,
        };
        assert!(!g.is_active());
        // Drop must be a no-op for a degraded guard. Implicit via end-of-scope.
    }
}
