//! Non-Linux stub for `DeviceReservation`.
//!
//! The org.freedesktop.ReserveDevice1 protocol is a Linux/D-Bus convention.
//! On macOS and Windows, `acquire()` always succeeds with a degraded guard
//! so that call sites stay portable; `is_active()` always returns `false`.

#[derive(Debug)]
pub struct DeviceReservation;

impl DeviceReservation {
    /// Acquire a reservation for the given ALSA hw: device string.
    ///
    /// On non-Linux platforms this always returns a degraded guard (no-op).
    pub fn acquire(_hw_device: &str, _app_device_name: &str) -> Result<Self, ReservationError> {
        Ok(Self)
    }

    /// Whether this guard holds an active D-Bus reservation.
    ///
    /// Always `false` on non-Linux platforms.
    pub fn is_active(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub enum ReservationError {
    Unsupported,
}

impl std::fmt::Display for ReservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "device reservation is not supported on this platform")
    }
}

impl std::error::Error for ReservationError {}
