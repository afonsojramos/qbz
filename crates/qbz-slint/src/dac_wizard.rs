//! HiFi Wizard (DAC setup) controller.
//!
//! Slice 6 (check step): runs the frontend-agnostic audio-stack probes
//! (`qbz_audio::health`) on open, maps them to per-distro copy-paste
//! remediations the check step renders, and recomputes them when the user
//! overrides the distro. Read-only — nothing here writes a system file or
//! opens a stream.

use std::sync::Mutex;

use qbz_audio::{AudioStackHealth, Distro, InitSystem};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, DacWizardState, RemediationRow};

// Last probe result, so a distro override recomputes commands without
// re-shelling on every dropdown change.
static LAST_HEALTH: Mutex<Option<AudioStackHealth>> = Mutex::new(None);

/// Synchronous part of opening the wizard: reset, fill the distro dropdown
/// (auto-detected, always overridable), and show a "checking…" state until the
/// async probe lands.
pub fn open_immediate(window: &AppWindow) {
    let st = window.global::<DacWizardState>();
    st.set_open(true);
    st.set_step(0);
    st.set_welcome_confirmed(false);

    let distro_opts: Vec<slint::SharedString> =
        Distro::ALL.iter().map(|d| d.label().into()).collect();
    st.set_distro_options(ModelRc::new(VecModel::from(distro_opts)));
    st.set_distro_index(qbz_audio::detect_distro().index() as i32);

    let init_opts: Vec<slint::SharedString> =
        InitSystem::ALL.iter().map(|i| i.label().into()).collect();
    st.set_init_options(ModelRc::new(VecModel::from(init_opts)));
    st.set_init_index(qbz_audio::detect_init().index() as i32);

    let sandbox = qbz_audio::detect_sandbox();
    st.set_sandboxed(sandbox != qbz_audio::Sandbox::None);
    st.set_sandbox_name(
        match sandbox {
            qbz_audio::Sandbox::Flatpak => "Flatpak",
            qbz_audio::Sandbox::Snap => "Snap",
            qbz_audio::Sandbox::None => "",
        }
        .into(),
    );

    st.set_health_ok(false);
    st.set_health_summary("Checking your audio stack…".into());
    st.set_remediations(ModelRc::new(VecModel::from(Vec::<RemediationRow>::new())));
}

/// Apply a completed health probe: cache it and re-render from the current
/// distro/init selections.
pub fn apply_health(window: &AppWindow, health: AudioStackHealth) {
    *LAST_HEALTH.lock().unwrap() = Some(health);
    recompute(window);
}

/// User overrode the distro (package manager) — recompute.
pub fn set_distro(window: &AppWindow, index: i32) {
    window.global::<DacWizardState>().set_distro_index(index);
    recompute(window);
}

/// User overrode the init system (service commands) — recompute.
pub fn set_init(window: &AppWindow, index: i32) {
    window.global::<DacWizardState>().set_init_index(index);
    recompute(window);
}

/// Rebuild the remediations from the cached probe + the current distro/init
/// dropdown selections (either of which the user can override).
fn recompute(window: &AppWindow) {
    let st = window.global::<DacWizardState>();
    let health = LAST_HEALTH
        .lock()
        .unwrap()
        .unwrap_or_else(qbz_audio::audio_stack_health);
    let distro = Distro::ALL
        .get(st.get_distro_index().max(0) as usize)
        .copied()
        .unwrap_or(Distro::Other);
    let init = InitSystem::ALL
        .get(st.get_init_index().max(0) as usize)
        .copied()
        .unwrap_or(InitSystem::Unknown);

    // In a sandbox the host probes are blind, so don't render a health verdict —
    // show reference setup commands for the chosen distro/init (Tauri-style,
    // which never probed either). The UI shows a sandbox info banner instead.
    let rows = if st.get_sandboxed() {
        st.set_health_ok(false);
        st.set_health_summary("".into());
        reference_commands(distro, init)
    } else {
        let r = remediations(health, distro, init);
        st.set_health_ok(health.is_ready());
        st.set_health_summary(if health.is_ready() {
            "Your audio stack is ready for bit-perfect playback.".into()
        } else {
            let n = r.len();
            format!(
                "{} item{} need attention before bit-perfect playback will work.",
                n,
                if n == 1 { "" } else { "s" }
            )
            .into()
        });
        r
    };
    let model: Vec<RemediationRow> = rows
        .into_iter()
        .map(|(caption, command)| RemediationRow {
            caption: caption.into(),
            command: command.into(),
        })
        .collect();
    st.set_remediations(ModelRc::new(VecModel::from(model)));
}

/// (caption, copy-paste command) per missing probe, for the given distro.
///
/// Service/restart commands are INIT-SYSTEM aware per distro (OpenRC on Gentoo,
/// runit on Void, systemd elsewhere), mirroring the Tauri DistroSelector
/// `restartCommands`. Installs and the restart are kept as separate blocks so
/// the multi-line Gentoo guidance never gets `&&`-joined.
fn remediations(h: AudioStackHealth, d: Distro, init: InitSystem) -> Vec<(String, String)> {
    // NixOS is declarative: you don't `apt/pacman install` pieces — you enable
    // the PipeWire module and rebuild. So collapse all the missing pieces into
    // one config block instead of per-package commands.
    if d == Distro::NixOS {
        if h.is_ready() {
            return Vec::new();
        }
        return vec![(
            "Enable PipeWire in your NixOS configuration".to_string(),
            NIXOS_PIPEWIRE_BLOCK.to_string(),
        )];
    }

    let mut out = Vec::new();
    let mut needs_restart = false;
    if !h.has_pw_dump {
        out.push((
            "Install the PipeWire tools (pw-dump)".to_string(),
            install(d, pkg_pw_tools(d)),
        ));
        needs_restart = true;
    }
    if !h.cpal_sees_pipewire {
        // THE Ubuntu no-list / no-playback bug: the ALSA->PipeWire bridge PCM.
        out.push((
            "Install the ALSA bridge so playback can reach PipeWire".to_string(),
            install(d, "pipewire-alsa"),
        ));
        needs_restart = true;
    }
    if !h.has_pactl {
        out.push((
            "Install the Pulse compatibility tools (optional fallback)".to_string(),
            install(d, pkg_pulse(d)),
        ));
        needs_restart = true;
    }
    if !h.any_devices {
        out.push((
            "No sinks detected — reinstall the ALSA UCM profiles, then reboot".to_string(),
            install_reinstall(d, "alsa-ucm-conf"),
        ));
    }
    // WirePlumber down, or we just installed something → (re)start the stack
    // with the ACTUAL init system running on this machine (not guessed from the
    // distro — Gentoo+systemd and Gentoo+OpenRC must differ).
    if !h.wireplumber_active || needs_restart {
        out.push((
            "(Re)start the PipeWire audio services".to_string(),
            restart_cmd(init).to_string(),
        ));
    }
    out
}

/// Init-system-aware "(re)start the audio services" command. PipeWire is a
/// user-session service, so only systemd has a first-class `--user` restart;
/// the others either use their user-service supervisor or a re-login.
fn restart_cmd(init: InitSystem) -> &'static str {
    match init {
        InitSystem::Systemd => "systemctl --user restart pipewire pipewire-pulse wireplumber",
        InitSystem::OpenRc => {
            "# OpenRC: PipeWire runs in your user session, not as an OpenRC service.\n\
             # Log out and back in to restart it."
        }
        InitSystem::Runit => {
            "sv restart pipewire wireplumber   # if set up as runit user services; otherwise log out and back in"
        }
        InitSystem::S6 => "# s6: restart via your supervision tree, or log out and back in",
        InitSystem::Dinit => "dinitctl restart pipewire wireplumber   # or log out and back in",
        InitSystem::Unknown => "# Restart PipeWire via your init system, or log out and back in",
    }
}

fn pkg_pw_tools(d: Distro) -> &'static str {
    match d {
        // Debian-family (incl. antiX) ship pw-* in pipewire-bin.
        Distro::Debian | Distro::Antix => "pipewire-bin",
        Distro::Fedora => "pipewire-utils",
        // Arch (incl. Artix) / openSUSE / Gentoo / Void ship pw-* with pipewire.
        _ => "pipewire",
    }
}

fn pkg_pulse(d: Distro) -> &'static str {
    match d {
        Distro::Debian | Distro::Antix => "pipewire-pulse pulseaudio-utils",
        Distro::Fedora => "pipewire-pulseaudio",
        _ => "pipewire-pulse",
    }
}

fn install(d: Distro, pkgs: &str) -> String {
    match d {
        // Package manager is a property of the distro family, NOT the init.
        Distro::Debian | Distro::Antix => format!("sudo apt install {pkgs}"),
        Distro::Fedora => format!("sudo dnf install {pkgs}"),
        Distro::Arch | Distro::Artix => format!("sudo pacman -S {pkgs}"),
        Distro::OpenSuse => format!("sudo zypper install {pkgs}"),
        Distro::Gentoo => format!("sudo emerge {pkgs}   # package name may differ on Gentoo"),
        Distro::Void => format!("sudo xbps-install -S {pkgs}"),
        // NixOS is special-cased in remediations(); this is an unreached fallback.
        Distro::NixOS => format!("# NixOS: add to configuration.nix (see the PipeWire block) — {pkgs}"),
        Distro::Other => format!("Install with your package manager: {pkgs}"),
    }
}

fn install_reinstall(d: Distro, pkg: &str) -> String {
    match d {
        Distro::Debian | Distro::Antix => format!("sudo apt install --reinstall {pkg}"),
        Distro::Fedora => format!("sudo dnf reinstall {pkg}"),
        _ => install(d, pkg),
    }
}

const NIXOS_PIPEWIRE_BLOCK: &str = "# /etc/nixos/configuration.nix:\n\
     services.pipewire = {\n\
     \u{20}\u{20}enable = true;\n\
     \u{20}\u{20}alsa.enable = true;\n\
     \u{20}\u{20}pulse.enable = true;\n\
     \u{20}\u{20}wireplumber.enable = true;\n\
     };\n\
     # then apply:\n\
     sudo nixos-rebuild switch";

/// Full reference setup commands for the chosen distro/init, shown when QBZ
/// can't probe the host (sandbox). Mirrors the Tauri DistroSelector, which
/// always showed per-distro install + restart commands (no probing).
fn reference_commands(d: Distro, init: InitSystem) -> Vec<(String, String)> {
    if d == Distro::NixOS {
        return vec![(
            "Enable PipeWire in your NixOS configuration".to_string(),
            NIXOS_PIPEWIRE_BLOCK.to_string(),
        )];
    }
    vec![
        (
            "Install the PipeWire audio stack".to_string(),
            install(d, full_stack_pkgs(d)),
        ),
        (
            "(Re)start the PipeWire audio services".to_string(),
            restart_cmd(init).to_string(),
        ),
    ]
}

/// The full recommended package set (incl. `pipewire-alsa`, the bit the old
/// Tauri list omitted — the cause of the Ubuntu empty-list bug).
fn full_stack_pkgs(d: Distro) -> &'static str {
    match d {
        Distro::Debian | Distro::Antix => {
            "pipewire pipewire-pulse pipewire-alsa wireplumber alsa-utils"
        }
        Distro::Fedora => "pipewire pipewire-pulseaudio pipewire-alsa wireplumber alsa-utils",
        Distro::Arch | Distro::Artix => {
            "pipewire pipewire-pulse pipewire-alsa wireplumber alsa-utils"
        }
        Distro::OpenSuse => "pipewire pipewire-pulseaudio pipewire-alsa wireplumber alsa-utils",
        Distro::Gentoo => "media-video/pipewire media-video/wireplumber media-sound/alsa-utils",
        Distro::Void => "pipewire wireplumber alsa-utils",
        Distro::NixOS => "",
        Distro::Other => "pipewire pipewire-pulse wireplumber alsa-utils",
    }
}
