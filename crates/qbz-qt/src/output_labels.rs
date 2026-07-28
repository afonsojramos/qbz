//! The two output LEDs of the now-playing quality stamp — a verbatim port of
//! `fn output_labels` in `crates/qbz/src/settings.rs`.
//!
//! `(backend_label, mode_label, backend_active, mode_active)` is derived
//! PURELY from the persisted `AudioSettings`; the "active" half is what
//! lights the LED. This module only READS AudioSettings — the audio path
//! (qbz-audio / qbz-player) is never touched from here.
//!
//! Published onto `QbzPlayer` (`np_output_*`) from `settings_qt::
//! publish_snapshot`, i.e. on every settings change and at boot. Same shape
//! as the Slint, where `apply_snapshot` mirrors the four values onto
//! `NowPlayingState` alongside `SettingsState`. No polling.

use cxx_qt_lib::QString;
use qbz_audio::backend::{AlsaPlugin, AudioBackendType};
use qbz_audio::settings::AudioSettings;

/// The four LED values. `*_active` = the LED is lit (a deliberate,
/// bit-perfect-capable route) rather than merely named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLabels {
    /// PIPEWIRE | ALSA | JACK | PULS | SYST | AUTO
    pub backend: &'static str,
    /// DACPASS | BITPERF | EXCL | DIRECT | LOCKED | ROUTED | SHARED | DEFAULT
    pub mode: &'static str,
    pub backend_active: bool,
    pub mode_active: bool,
}

impl Default for OutputLabels {
    fn default() -> Self {
        // The Slint NowPlayingState defaults (state.slint).
        Self {
            backend: "SYST",
            mode: "DEFAULT",
            backend_active: false,
            mode_active: false,
        }
    }
}

/// `settings.rs::output_labels`, ported verbatim — this mapping IS the
/// contract for the two LEDs; do not "improve" it.
pub fn output_labels(audio: &AudioSettings) -> OutputLabels {
    let (backend, backend_active) = match audio.backend_type {
        Some(AudioBackendType::PipeWire) => ("PIPEWIRE", true),
        Some(AudioBackendType::Alsa) => ("ALSA", true),
        Some(AudioBackendType::Jack) => ("JACK", true),
        Some(AudioBackendType::Pulse) => ("PULS", true),
        Some(AudioBackendType::SystemDefault) => ("SYST", false),
        None => ("AUTO", false),
    };
    let (mode, mode_active) = match audio.backend_type {
        Some(AudioBackendType::PipeWire) => {
            if audio.dac_passthrough {
                ("DACPASS", true)
            } else if audio.pw_force_bitperfect {
                ("BITPERF", true)
            } else {
                ("SHARED", false)
            }
        }
        Some(AudioBackendType::Alsa) => match audio.alsa_plugin {
            Some(AlsaPlugin::Hw) => {
                if audio.exclusive_mode {
                    ("EXCL", true)
                } else {
                    ("DIRECT", true)
                }
            }
            _ => ("SHARED", false),
        },
        Some(AudioBackendType::Jack) => {
            if audio.reserve_dac_while_running {
                ("LOCKED", true)
            } else {
                ("ROUTED", false)
            }
        }
        Some(AudioBackendType::Pulse) => ("SHARED", false),
        Some(AudioBackendType::SystemDefault) | None => ("DEFAULT", false),
    };
    OutputLabels {
        backend,
        mode,
        backend_active,
        mode_active,
    }
}

/// Derive + push the four LED values onto the player bridge (Qt-thread hop).
/// Called from `settings_qt::publish_snapshot`, so the labels follow an audio
/// settings change with no poll of their own.
pub fn publish(audio: &AudioSettings) {
    let l = output_labels(audio);
    crate::player_bridge::ui(move |mut b| {
        b.as_mut().set_np_output_backend_label(QString::from(l.backend));
        b.as_mut().set_np_output_mode_label(QString::from(l.mode));
        b.as_mut().set_np_output_backend_active(l.backend_active);
        b.as_mut().set_np_output_mode_active(l.mode_active);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(backend: Option<AudioBackendType>) -> AudioSettings {
        AudioSettings {
            backend_type: backend,
            ..AudioSettings::default()
        }
    }

    #[test]
    fn auto_backend_is_the_unlit_default() {
        let l = output_labels(&settings(None));
        assert_eq!((l.backend, l.mode), ("AUTO", "DEFAULT"));
        assert!(!l.backend_active && !l.mode_active);
    }

    #[test]
    fn pipewire_modes() {
        let mut s = settings(Some(AudioBackendType::PipeWire));
        s.dac_passthrough = false;
        s.pw_force_bitperfect = false;
        assert_eq!(output_labels(&s).mode, "SHARED");
        assert!(!output_labels(&s).mode_active);
        s.pw_force_bitperfect = true;
        assert_eq!(output_labels(&s).mode, "BITPERF");
        // dac_passthrough wins over pw_force_bitperfect.
        s.dac_passthrough = true;
        let l = output_labels(&s);
        assert_eq!((l.backend, l.mode), ("PIPEWIRE", "DACPASS"));
        assert!(l.backend_active && l.mode_active);
    }

    #[test]
    fn alsa_hw_is_direct_or_exclusive_and_plug_is_shared() {
        let mut s = settings(Some(AudioBackendType::Alsa));
        s.alsa_plugin = Some(AlsaPlugin::Hw);
        s.exclusive_mode = false;
        let l = output_labels(&s);
        assert_eq!((l.backend, l.mode), ("ALSA", "DIRECT"));
        assert!(l.mode_active);
        s.exclusive_mode = true;
        assert_eq!(output_labels(&s).mode, "EXCL");
        s.alsa_plugin = Some(AlsaPlugin::PlugHw);
        let l = output_labels(&s);
        assert_eq!(l.mode, "SHARED");
        assert!(!l.mode_active);
    }

    #[test]
    fn jack_reservation_lights_the_mode_led() {
        let mut s = settings(Some(AudioBackendType::Jack));
        s.reserve_dac_while_running = false;
        assert_eq!(output_labels(&s).mode, "ROUTED");
        assert!(!output_labels(&s).mode_active);
        s.reserve_dac_while_running = true;
        assert_eq!(output_labels(&s).mode, "LOCKED");
        assert!(output_labels(&s).mode_active);
    }

    #[test]
    fn pulse_and_system_default() {
        let l = output_labels(&settings(Some(AudioBackendType::Pulse)));
        assert_eq!((l.backend, l.mode), ("PULS", "SHARED"));
        assert!(l.backend_active && !l.mode_active);
        let l = output_labels(&settings(Some(AudioBackendType::SystemDefault)));
        assert_eq!((l.backend, l.mode), ("SYST", "DEFAULT"));
        assert!(!l.backend_active && !l.mode_active);
    }
}
