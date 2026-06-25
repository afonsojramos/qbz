//! HiFi Wizard (DAC setup) controller.
//!
//! Slice 6 (check step): runs the frontend-agnostic audio-stack probes
//! (`qbz_audio::health`) on open, maps them to per-distro copy-paste
//! remediations the check step renders, and recomputes them when the user
//! overrides the distro. Read-only — nothing here writes a system file or
//! opens a stream.

use std::sync::Mutex;

use qbz_audio::{AudioStackHealth, Distro, InitSystem};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, DacCandidateRow, DacConfigRow, DacWizardState, RemediationRow};

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
    st.set_health_summary(qbz_i18n::t("Checking your audio stack…").into());
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
            qbz_i18n::t("Your audio stack is ready for bit-perfect playback.").into()
        } else {
            let n = r.len();
            qbz_i18n::tf(
                "{} item needs attention before bit-perfect playback will work.",
                "{} items need attention before bit-perfect playback will work.",
                n as i64,
                &[&n.to_string()],
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
            qbz_i18n::t("Enable PipeWire in your NixOS configuration"),
            NIXOS_PIPEWIRE_BLOCK.to_string(),
        )];
    }

    let mut out = Vec::new();
    let mut needs_restart = false;
    if !h.has_pw_dump {
        out.push((
            qbz_i18n::t("Install the PipeWire tools (pw-dump)"),
            install(d, pkg_pw_tools(d)),
        ));
        needs_restart = true;
    }
    if !h.cpal_sees_pipewire {
        // THE Ubuntu no-list / no-playback bug: the ALSA->PipeWire bridge PCM.
        out.push((
            qbz_i18n::t("Install the ALSA bridge so playback can reach PipeWire"),
            install(d, "pipewire-alsa"),
        ));
        needs_restart = true;
    }
    if !h.has_pactl {
        out.push((
            qbz_i18n::t("Install the Pulse compatibility tools (optional fallback)"),
            install(d, pkg_pulse(d)),
        ));
        needs_restart = true;
    }
    if !h.any_devices {
        out.push((
            qbz_i18n::t("No sinks detected — reinstall the ALSA UCM profiles, then reboot"),
            install_reinstall(d, "alsa-ucm-conf"),
        ));
    }
    // WirePlumber down, or we just installed something → (re)start the stack
    // with the ACTUAL init system running on this machine (not guessed from the
    // distro — Gentoo+systemd and Gentoo+OpenRC must differ).
    if !h.wireplumber_active || needs_restart {
        out.push((
            qbz_i18n::t("(Re)start the PipeWire audio services"),
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
            qbz_i18n::t("Enable PipeWire in your NixOS configuration"),
            NIXOS_PIPEWIRE_BLOCK.to_string(),
        )];
    }
    vec![
        (
            qbz_i18n::t("Install the PipeWire audio stack"),
            install(d, full_stack_pkgs(d)),
        ),
        (
            qbz_i18n::t("(Re)start the PipeWire audio services"),
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

// ── Slice 7: select-dacs (auto-detect + manual escape hatch) ───────────────

/// Plain, `Send` candidate produced on the worker thread.
pub struct DacCandidateData {
    id: String,
    description: String,
    bus: String,
    is_default: bool,
    looks_like_dac: bool,
    rates_label: String,
}

/// Immediate UI feedback before the (blocking) enumeration runs.
pub fn begin_detect(window: &AppWindow) {
    window.global::<DacWizardState>().set_detecting(true);
}

/// Heavy work (enumerate sinks via the pw-dump-robust path + probe rates for
/// the likely DACs). Runs on a blocking thread; returns plain data.
pub fn detect_blocking() -> Vec<DacCandidateData> {
    let devices = qbz_audio::backend::BackendManager::create_backend(
        qbz_audio::backend::AudioBackendType::PipeWire,
    )
    .and_then(|b| b.enumerate_devices())
    .unwrap_or_default();

    let mut out = Vec::new();
    for d in devices {
        let bus = d.device_bus.unwrap_or_default();
        let looks_like_dac = d.is_hardware && (bus == "usb" || bus == "pci");
        // Only probe rates for likely DACs (skip virtual/monitor sinks).
        let rates_label = if looks_like_dac {
            format_rates(&qbz_audio::query_dac_capabilities(&d.id).sample_rates)
        } else {
            String::new()
        };
        let description = if d.name.is_empty() { d.id.clone() } else { d.name };
        out.push(DacCandidateData {
            id: d.id,
            description,
            bus,
            is_default: d.is_default,
            looks_like_dac,
            rates_label,
        });
    }
    out
}

/// Apply enumerated candidates to the state. Pre-selects the likely DACs; if
/// nothing enumerated, flips `has-enumeration` off so the manual escape hatch
/// shows.
pub fn apply_candidates(window: &AppWindow, data: Vec<DacCandidateData>) {
    let st = window.global::<DacWizardState>();
    let rows: Vec<DacCandidateRow> = data
        .iter()
        .map(|d| DacCandidateRow {
            id: d.id.clone().into(),
            description: d.description.clone().into(),
            bus: d.bus.clone().into(),
            is_default: d.is_default,
            looks_like_dac: d.looks_like_dac,
            checked: d.looks_like_dac,
            rates_label: d.rates_label.clone().into(),
        })
        .collect();
    let any = rows.iter().any(|r| r.checked);
    st.set_has_enumeration(!data.is_empty());
    st.set_any_dac_selected(any);
    st.set_candidates(ModelRc::new(VecModel::from(rows)));
    st.set_detecting(false);
}

/// Flip one candidate's checkbox + recompute the Next gate.
pub fn toggle_dac(window: &AppWindow, index: i32) {
    let st = window.global::<DacWizardState>();
    let model = st.get_candidates();
    if let Some(vm) = model
        .as_any()
        .downcast_ref::<VecModel<DacCandidateRow>>()
    {
        if let Some(mut row) = vm.row_data(index.max(0) as usize) {
            row.checked = !row.checked;
            vm.set_row_data(index.max(0) as usize, row);
        }
    }
    let any = (0..model.row_count()).any(|i| model.row_data(i).map(|r| r.checked).unwrap_or(false));
    st.set_any_dac_selected(any);
}

/// Validate a manually-pasted node.name (escape hatch). 1:1 with the Tauri
/// `validateNodeName` / `detectDacType`.
pub fn validate_manual(window: &AppWindow, text: &str) {
    let st = window.global::<DacWizardState>();
    st.set_manual_node_name(text.into());
    st.set_manual_valid(validate_node_name(text));
    st.set_manual_dac_type(detect_dac_type(text).into());
}

fn validate_node_name(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty() && (t.contains("alsa_output") || t.contains("alsa_input"))
}

fn detect_dac_type(name: &str) -> &'static str {
    let l = name.to_lowercase();
    if l.contains("usb-") || l.contains(".usb") {
        "usb"
    } else if l.contains("pci-") || l.contains(".pci") {
        "pci"
    } else if l.contains("bluez") || l.contains("bluetooth") {
        "bluetooth"
    } else if l.contains("virtual") || l.contains("null") || l.contains("dummy") {
        "virtual"
    } else {
        "unknown"
    }
}

/// "44.1 / 96 / 192 kHz" from a rate list (kHz, .1 only when non-integer).
fn format_rates(rates: &[u32]) -> String {
    if rates.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = rates
        .iter()
        .map(|&r| {
            if r % 1000 == 0 {
                format!("{}", r / 1000)
            } else {
                format!("{:.1}", r as f64 / 1000.0)
            }
        })
        .collect();
    format!("{} kHz", parts.join(" / "))
}

#[cfg(test)]
mod slice7_tests {
    use super::*;

    #[test]
    fn validates_node_names_like_tauri() {
        assert!(validate_node_name("alsa_output.usb-Cambridge-00.analog-stereo"));
        assert!(validate_node_name("alsa_input.pci-0000_00.analog-stereo"));
        assert!(!validate_node_name(""));
        assert!(!validate_node_name("   "));
        assert!(!validate_node_name("bluez_output.AA_BB"));
    }

    #[test]
    fn detects_dac_type() {
        assert_eq!(detect_dac_type("alsa_output.usb-Cambridge-00.analog-stereo"), "usb");
        assert_eq!(detect_dac_type("alsa_output.pci-0000_00_1f.3.analog-stereo"), "pci");
        assert_eq!(detect_dac_type("bluez_output.AA"), "bluetooth");
        assert_eq!(detect_dac_type("alsa_output.virtual-dummy"), "virtual");
        assert_eq!(detect_dac_type("something.else"), "unknown");
    }

    #[test]
    fn formats_rates_khz() {
        assert_eq!(format_rates(&[44100, 96000, 192000]), "44.1 / 96 / 192 kHz");
        assert_eq!(format_rates(&[]), "");
    }
}

// ── Slice 9: self-service playback test (N6 read-back) ─────────────────────

/// One curated test track (owner-provided). Resolved by id-hint first, then by
/// "artist title" search if the id 404s (a pulled license) — never raw-id-only.
pub struct TestSeed {
    pub depth: u32,
    pub rate: f64,
    pub id_hint: u64,
    pub artist: &'static str,
    pub title: &'static str,
}

pub const TEST_SEEDS: [TestSeed; 4] = [
    TestSeed { depth: 16, rate: 44100.0, id_hint: 19301386, artist: "George Harrison", title: "My Sweet Lord" },
    TestSeed { depth: 24, rate: 44100.0, id_hint: 266725027, artist: "Billie Eilish", title: "LUNCH" },
    TestSeed { depth: 24, rate: 96000.0, id_hint: 126886854, artist: "Iron Maiden", title: "Stratego" },
    TestSeed { depth: 24, rate: 192000.0, id_hint: 52265, artist: "Toto", title: "Africa" },
];

/// True if a resolved track matches this seed's family (rate + bit depth — the
/// two 44.1 seeds only differ by depth).
pub fn track_matches_seed(track: &qbz_models::Track, seed: &TestSeed) -> bool {
    let rate_ok = track
        .maximum_sampling_rate
        .map(|r| (r * 1000.0 - seed.rate).abs() < 1.0 || (r - seed.rate).abs() < 1.0)
        .unwrap_or(false);
    let depth_ok = track.maximum_bit_depth.map(|d| d == seed.depth).unwrap_or(false);
    rate_ok && depth_ok
}

/// Resolved test tracks, kept so the user can jump straight to any of them
/// (re-set the queue at the chosen index via the working play path).
static TEST_TRACKS: Mutex<Vec<qbz_models::Track>> = Mutex::new(Vec::new());

pub fn stash_test_tracks(tracks: Vec<qbz_models::Track>) {
    *TEST_TRACKS.lock().unwrap() = tracks;
}

pub fn test_tracks() -> Vec<qbz_models::Track> {
    TEST_TRACKS.lock().unwrap().clone()
}

/// Start the test: show the "playing" state. The read-back probes whichever
/// DAC is actively playing (scan), so no node needs to be stashed.
pub fn begin_test(window: &AppWindow) {
    let st = window.global::<DacWizardState>();
    st.set_test_playing(true);
    st.set_test_rate_matched(false);
    st.set_test_requested_label(qbz_i18n::t("Starting…").into());
    st.set_test_negotiated_label("".into());
}

pub fn end_test(window: &AppWindow) {
    window.global::<DacWizardState>().set_test_playing(false);
}

/// Guardrail: "Use my current queue" with an empty queue — show a hint instead
/// of starting a read-back that would just sit on "Nothing playing".
pub fn queue_empty_notice(window: &AppWindow) {
    let st = window.global::<DacWizardState>();
    st.set_test_playing(false);
    st.set_test_rate_matched(false);
    st.set_test_negotiated_label("".into());
    st.set_test_requested_label(
        qbz_i18n::t("Your queue is empty — add some tracks first, or press Play test.").into(),
    );
}

/// Apply one poll: the rate QBZ requested vs the DAC's real negotiated rate (N6).
pub fn apply_poll(
    window: &AppWindow,
    requested_rate: u32,
    requested_bits: u32,
    negotiated: Option<qbz_audio::NegotiatedRate>,
) {
    let st = window.global::<DacWizardState>();
    st.set_test_requested_label(if requested_rate > 0 {
        qbz_i18n::t_args(
            "QBZ requesting {} · {}-bit",
            &[&khz(requested_rate), &requested_bits.to_string()],
        )
        .into()
    } else {
        qbz_i18n::t("Nothing playing").into()
    });
    match negotiated {
        Some(n) => {
            // The DAC's REAL hardware params (N6): rate + ALSA container format
            // (e.g. S32_LE = 24-bit in a 32-bit frame) + channels. This is the
            // bit-perfect proof — exactly what the hardware is clocked at.
            st.set_test_negotiated_label(
                qbz_i18n::t_args(
                    "DAC: {} · {} · {} ch",
                    &[&khz(n.sample_rate), &n.format, &n.channels.to_string()],
                )
                .into(),
            );
            // Truth signal: the DAC's real clock matches what QBZ asked for.
            st.set_test_rate_matched(requested_rate > 0 && n.sample_rate == requested_rate);
        }
        None => {
            st.set_test_negotiated_label(qbz_i18n::t("Waiting for the DAC to start playing…").into());
            st.set_test_rate_matched(false);
        }
    }
}

fn khz(hz: u32) -> String {
    if hz % 1000 == 0 {
        format!("{} kHz", hz / 1000)
    } else {
        format!("{:.1} kHz", hz as f64 / 1000.0)
    }
}

// ── Slice 10: review-and-apply (per-DAC config generation) ─────────────────

/// Plain, `Send` per-DAC generated config (built on a worker thread).
pub struct DacConfigData {
    name: String,
    node_name: String,
    pipewire_conf: String,
    pulse_conf: String,
    wireplumber_conf: String,
}

/// (node_name, display_name) for every checked candidate, or a valid manual one.
pub fn checked_dacs(window: &AppWindow) -> Vec<(String, String)> {
    let st = window.global::<DacWizardState>();
    let model = st.get_candidates();
    let mut out = Vec::new();
    for i in 0..model.row_count() {
        if let Some(r) = model.row_data(i) {
            if r.checked {
                out.push((r.id.to_string(), r.description.to_string()));
            }
        }
    }
    if out.is_empty() {
        let manual = st.get_manual_node_name().to_string();
        if !manual.trim().is_empty() && st.get_manual_valid() {
            out.push((manual.clone(), manual));
        }
    }
    out
}

/// Re-probe rates + build the three config snippets per DAC (blocking).
pub fn gen_configs_blocking(dacs: Vec<(String, String)>) -> Vec<DacConfigData> {
    dacs.into_iter()
        .map(|(node_name, name)| {
            let rates = qbz_audio::query_dac_capabilities(&node_name).sample_rates;
            let short = short_name(&name, &node_name);
            DacConfigData {
                pipewire_conf: pipewire_conf(&short, &rates),
                pulse_conf: pulse_conf(&short),
                wireplumber_conf: wireplumber_conf(&short, &node_name, &rates, &name),
                name,
                node_name,
            }
        })
        .collect()
}

/// Push generated configs + backup/restart/paths to the state.
pub fn apply_configs(window: &AppWindow, data: Vec<DacConfigData>) {
    let st = window.global::<DacWizardState>();
    let single = data.len() == 1;
    let rows: Vec<DacConfigRow> = data
        .iter()
        .map(|d| DacConfigRow {
            name: d.name.clone().into(),
            node_name: d.node_name.clone().into(),
            pipewire_conf: d.pipewire_conf.clone().into(),
            pulse_conf: d.pulse_conf.clone().into(),
            wireplumber_conf: d.wireplumber_conf.clone().into(),
            expanded: single, // one DAC → expanded; multiple → collapsed accordions
        })
        .collect();
    let mut paths: Vec<slint::SharedString> = Vec::new();
    for d in &data {
        let short = short_name(&d.name, &d.node_name);
        paths.push(format!("~/.config/pipewire/pipewire.conf.d/99-qbz-dac-{short}.conf").into());
        paths.push(format!("~/.config/pipewire/client.conf.d/99-qbz-bitperfect-{short}.conf").into());
        paths
            .push(format!("~/.config/wireplumber/wireplumber.conf.d/99-qbz-dac-{short}.conf").into());
    }
    st.set_dac_configs(ModelRc::new(VecModel::from(rows)));
    st.set_created_paths(ModelRc::new(VecModel::from(paths)));
    st.set_backup_cmd(BACKUP_CMD.into());
    let init = InitSystem::ALL
        .get(st.get_init_index().max(0) as usize)
        .copied()
        .unwrap_or(InitSystem::Unknown);
    st.set_restart_cmd(restart_cmd(init).into());
}

/// Collapse/expand one DAC's generated-config accordion.
pub fn toggle_config(window: &AppWindow, index: i32) {
    let model = window.global::<DacWizardState>().get_dac_configs();
    if let Some(vm) = model.as_any().downcast_ref::<VecModel<DacConfigRow>>() {
        if let Some(mut row) = vm.row_data(index.max(0) as usize) {
            row.expanded = !row.expanded;
            vm.set_row_data(index.max(0) as usize, row);
        }
    }
}

const BACKUP_CMD: &str = "BACKUP=~/.config/qbz/backups/pipewire-$(date +%Y%m%d-%H%M%S)\nmkdir -p \"$BACKUP\"\ncp -a ~/.config/pipewire \"$BACKUP/\" 2>/dev/null || true\ncp -a ~/.config/wireplumber \"$BACKUP/\" 2>/dev/null || true\necho \"Backup created at: $BACKUP\"";

/// A short, filename-safe DAC name: slug of the description, else the node.name.
fn short_name(name: &str, node_name: &str) -> String {
    let slug = slugify(name);
    if !slug.is_empty() {
        return slug;
    }
    let nslug = slugify(node_name);
    if nslug.is_empty() {
        "dac".to_string()
    } else {
        nslug
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn rates_list(rates: &[u32]) -> String {
    if rates.is_empty() {
        "44100 48000 88200 96000 176400 192000".to_string()
    } else {
        rates
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn pipewire_conf(short: &str, rates: &[u32]) -> String {
    let rates = rates_list(rates);
    [
        "mkdir -p ~/.config/pipewire/pipewire.conf.d".to_string(),
        format!("cat > ~/.config/pipewire/pipewire.conf.d/99-qbz-dac-{short}.conf << 'EOF'"),
        "# QBZ DAC Setup - Sample Rate Switching".to_string(),
        "context.properties = {".to_string(),
        format!("  default.clock.allowed-rates = [ {rates} ]"),
        "}".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

fn pulse_conf(short: &str) -> String {
    [
        "mkdir -p ~/.config/pipewire/client.conf.d".to_string(),
        format!("cat > ~/.config/pipewire/client.conf.d/99-qbz-bitperfect-{short}.conf << 'EOF'"),
        "# QBZ DAC Setup - Per-App Bit-Perfect".to_string(),
        "stream.rules = [".to_string(),
        "  {".to_string(),
        "    matches = [".to_string(),
        "      { application.process.binary = \"qbz\" }".to_string(),
        "      { application.name = \"PipeWire ALSA [qbz]\" }".to_string(),
        "    ]".to_string(),
        "    actions = { update-props = { resample.disable = true, channelmix.disable = true } }"
            .to_string(),
        "  }".to_string(),
        "]".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

fn wireplumber_conf(short: &str, node_name: &str, rates: &[u32], description: &str) -> String {
    let rates = rates_list(rates);
    [
        "mkdir -p ~/.config/wireplumber/wireplumber.conf.d".to_string(),
        format!("cat > ~/.config/wireplumber/wireplumber.conf.d/99-qbz-dac-{short}.conf << 'EOF'"),
        format!("# QBZ DAC Setup - {description}"),
        "monitor.alsa.rules = [".to_string(),
        "  {".to_string(),
        "    matches = [".to_string(),
        format!("      {{ node.name = \"{node_name}\", media.class = \"Audio/Sink\" }}"),
        "    ]".to_string(),
        "    actions = {".to_string(),
        "      update-props = {".to_string(),
        format!("        audio.allowed-rates = [ {rates} ]"),
        "        resample.disable = true".to_string(),
        "        channelmix.disable = true".to_string(),
        "      }".to_string(),
        "    }".to_string(),
        "  }".to_string(),
        "]".to_string(),
        "EOF".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod slice10_tests {
    use super::*;

    #[test]
    fn slugifies_descriptions() {
        assert_eq!(slugify("DacMagic Plus Analog Stereo"), "dacmagic-plus-analog-stereo");
        assert_eq!(slugify("Built-in Audio Analog Stereo"), "built-in-audio-analog-stereo");
        assert_eq!(slugify("  weird__name!! "), "weird-name");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn wireplumber_conf_pins_node_and_rates() {
        let c = wireplumber_conf("dacmagic", "alsa_output.usb-x.analog-stereo", &[44100, 192000], "DacMagic");
        assert!(c.contains("node.name = \"alsa_output.usb-x.analog-stereo\""));
        assert!(c.contains("audio.allowed-rates = [ 44100 192000 ]"));
        assert!(c.contains("99-qbz-dac-dacmagic.conf"));
        assert!(c.contains("resample.disable = true"));
    }
}
