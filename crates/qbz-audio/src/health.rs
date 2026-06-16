//! Audio-stack health probes + distro detection (HiFi wizard Slice 6 / check step).
//!
//! Cheap, read-only shell probes that tell the wizard what (if anything) is
//! missing on a Linux audio stack, plus `/etc/os-release` distro detection so
//! the check step can show the right `apt`/`dnf`/`pacman` remediation. None of
//! this opens a stream — purely diagnostic.

use std::process::Command;

/// Result of the audio-stack probes. All best-effort; a failed probe reads as
/// `false` (the wizard then surfaces the matching remediation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioStackHealth {
    /// WirePlumber session manager is running (`systemctl --user is-active`).
    pub wireplumber_active: bool,
    /// `pw-dump` is installed (native enumeration; `pipewire-bin`).
    pub has_pw_dump: bool,
    /// CPAL can see PipeWire through the ALSA bridge PCM (`aplay -L` lists
    /// `pipewire`). This needs `pipewire-alsa` and is required for PLAYBACK
    /// (the stream is opened via CPAL), not just enumeration.
    pub cpal_sees_pipewire: bool,
    /// `pactl` is available (Pulse compat path; `pulseaudio-utils`).
    pub has_pactl: bool,
    /// At least one audio sink is visible to `pw-dump`.
    pub any_devices: bool,
}

impl AudioStackHealth {
    /// Everything the wizard needs for bit-perfect playback is present.
    pub fn is_ready(&self) -> bool {
        self.wireplumber_active && self.cpal_sees_pipewire && self.any_devices
    }
}

/// True if `sh -c "<probe>"` exits 0.
fn sh_ok(probe: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run the audio-stack probes. Linux-only meaningfully; elsewhere everything
/// reads false except where trivially true.
pub fn audio_stack_health() -> AudioStackHealth {
    // systemd path first; fall back to a process check so non-systemd inits
    // (Gentoo/OpenRC, Void/runit) don't read as "WirePlumber down".
    let wireplumber_active = Command::new("systemctl")
        .args(["--user", "is-active", "wireplumber"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
        || sh_ok("pgrep -x wireplumber >/dev/null 2>&1");

    AudioStackHealth {
        wireplumber_active,
        has_pw_dump: sh_ok("command -v pw-dump >/dev/null 2>&1"),
        // `^pipewire$` line in `aplay -L` = the ALSA->PipeWire bridge PCM.
        cpal_sees_pipewire: sh_ok("aplay -L 2>/dev/null | grep -q '^pipewire$'"),
        has_pactl: sh_ok("command -v pactl >/dev/null 2>&1"),
        any_devices: sh_ok("pw-dump 2>/dev/null | grep -q 'Audio/Sink'"),
    }
}

/// Linux distribution family — drives the per-distro install commands. Order
/// matches the wizard's distro dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Debian,
    Fedora,
    Arch,
    OpenSuse,
    Gentoo,
    Void,
    Other,
}

impl Distro {
    /// Dropdown order (index = position here).
    pub const ALL: [Distro; 7] = [
        Distro::Debian,
        Distro::Fedora,
        Distro::Arch,
        Distro::OpenSuse,
        Distro::Gentoo,
        Distro::Void,
        Distro::Other,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&d| d == self).unwrap_or(6)
    }

    /// Human label for the dropdown (mirrors the Tauri DistroSelector).
    pub fn label(self) -> &'static str {
        match self {
            Distro::Debian => "Ubuntu / Debian / Mint / Pop!_OS",
            Distro::Fedora => "Fedora / RHEL",
            Distro::Arch => "Arch / Manjaro / EndeavourOS",
            Distro::OpenSuse => "openSUSE",
            Distro::Gentoo => "Gentoo / Funtoo",
            Distro::Void => "Void Linux",
            Distro::Other => "Other",
        }
    }
}

/// Detect the distro from `/etc/os-release`, defaulting to `Other`.
pub fn detect_distro() -> Distro {
    std::fs::read_to_string("/etc/os-release")
        .map(|c| parse_distro(&c))
        .unwrap_or(Distro::Other)
}

/// Pure `/etc/os-release` classifier (testable). Reads `ID` then `ID_LIKE`.
fn parse_distro(os_release: &str) -> Distro {
    let mut id = String::new();
    let mut id_like = String::new();
    for line in os_release.lines() {
        // os-release values may be bare, double-quoted, or single-quoted
        // (Gentoo uses single quotes), so strip both quote styles.
        if let Some(v) = line.strip_prefix("ID=") {
            id = v.trim().trim_matches(|c| c == '"' || c == '\'').to_lowercase();
        } else if let Some(v) = line.strip_prefix("ID_LIKE=") {
            id_like = v.trim().trim_matches(|c| c == '"' || c == '\'').to_lowercase();
        }
    }
    let hay = format!("{} {}", id, id_like);
    let has = |needle: &str| hay.contains(needle);
    if has("ubuntu") || has("debian") || has("mint") || has("pop") {
        Distro::Debian
    } else if has("fedora") || has("rhel") || has("centos") {
        Distro::Fedora
    } else if has("arch") || has("manjaro") || has("endeavour") {
        Distro::Arch
    } else if has("suse") {
        Distro::OpenSuse
    } else if has("gentoo") {
        Distro::Gentoo
    } else if has("void") {
        Distro::Void
    } else {
        Distro::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_ubuntu_as_debian_family() {
        let os = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\n";
        assert_eq!(parse_distro(os), Distro::Debian);
    }

    #[test]
    fn classifies_via_id_like_when_id_unknown() {
        // Pop!_OS-style: ID=pop, ID_LIKE="ubuntu debian"
        let os = "ID=pop\nID_LIKE=\"ubuntu debian\"\n";
        assert_eq!(parse_distro(os), Distro::Debian);
        // EndeavourOS: ID=endeavouros, ID_LIKE=arch
        let os2 = "ID=endeavouros\nID_LIKE=arch\n";
        assert_eq!(parse_distro(os2), Distro::Arch);
    }

    #[test]
    fn classifies_fedora_arch_suse_gentoo_void_other() {
        assert_eq!(parse_distro("ID=fedora\n"), Distro::Fedora);
        assert_eq!(parse_distro("ID=arch\n"), Distro::Arch);
        assert_eq!(parse_distro("ID=opensuse-tumbleweed\nID_LIKE=\"suse opensuse\"\n"), Distro::OpenSuse);
        assert_eq!(parse_distro("ID=gentoo\n"), Distro::Gentoo);
        // Gentoo's real os-release single-quotes the value.
        assert_eq!(parse_distro("ID='gentoo'\n"), Distro::Gentoo);
        assert_eq!(parse_distro("ID=void\n"), Distro::Void);
        assert_eq!(parse_distro("ID=nixos\n"), Distro::Other);
        assert_eq!(parse_distro(""), Distro::Other);
    }

    #[test]
    fn distro_index_round_trips() {
        for d in Distro::ALL {
            assert_eq!(Distro::ALL[d.index()], d);
        }
    }
}
