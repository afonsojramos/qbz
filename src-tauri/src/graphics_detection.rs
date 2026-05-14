//! GPU enumeration for the Settings > Graphics rendering-GPU picker.
//!
//! Cross-references the PCI devices visible under `/sys/class/drm/` (real
//! physical GPUs) with the EGL vendor JSONs available under
//! `/usr/share/glvnd/egl_vendor.d/` (stacks the running process can
//! actually use). The intersection is what we expose to the user.
//!
//! Inside Flatpak/Snap the `/sys` path is still mounted (read-only) so
//! PCI detection works; the `/usr/share/glvnd/...` view reflects the
//! sandbox's own vendor JSONs which is the correct thing to read — that's
//! what the WebKit process inside the sandbox can load.
//!
//! Companion to `autoconfig_graphics` (which decides "what config should
//! we recommend") — this module answers "what hardware/stacks do we
//! actually have".
//!
//! Cross-platform contract: the public API (`enumerate_gpus`,
//! `env_vars_for_preferred_gpu`, `PreferredGpu`) compiles on every
//! platform. macOS/Windows fall through to empty enumerations and a
//! no-op env-var application — the dropdown shows only the Software
//! entry there, and the preference can still round-trip through
//! storage without breaking the build.

use serde::Serialize;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;

/// PCI vendor IDs we recognize.
const VENDOR_INTEL: &str = "0x8086";
const VENDOR_AMD: &str = "0x1002";
const VENDOR_NVIDIA: &str = "0x10de";

/// Coarse classification of a GPU's role on the system. Drives the
/// label the Settings dropdown shows ("Integrated" vs "Discrete") and
/// the default `preferred_gpu` selection when the user hasn't picked
/// one yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuKind {
    Integrated,
    Discrete,
    /// Pure-software rendering (llvmpipe / cpu fallback). Not a real
    /// device — present in the list only so the dropdown can offer
    /// "Software (CPU)" as an explicit choice.
    Software,
}

/// One detected GPU with everything the picker needs to render and the
/// env-var application path needs to act on the user's choice.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DetectedGpu {
    /// Stable identifier across launches: the PCI slot string for real
    /// devices (e.g. "0000:01:00.0") or a sentinel for software.
    pub id: String,
    pub vendor: String,
    /// Human-readable model where available (from drm
    /// `device/product_name` or vendor heuristic fallback).
    pub name: String,
    /// PCI ID as `vendor:device` for diagnostics. Stripped of the `0x`
    /// prefix so it reads like `8086:7d55`.
    pub pci_id: Option<String>,
    pub kind: GpuKind,
    /// Absolute path of the EGL vendor JSON that exposes this GPU's
    /// stack to the running process, if one is available. None means
    /// the user's sandbox/install doesn't have the vendor library
    /// reachable, in which case picking this GPU would fall back to
    /// the system default.
    pub egl_vendor_json: Option<String>,
    /// True when the GPU is BOTH physically present AND reachable via
    /// an EGL vendor JSON. The picker should grey out (or omit)
    /// entries where this is false.
    pub is_usable: bool,
}

/// Enumerate every GPU the user could pick. Always returns at least
/// the `Software` entry so the dropdown is never empty on systems with
/// no detectable hardware.
pub fn enumerate_gpus() -> Vec<DetectedGpu> {
    let physical = detect_physical_gpus();
    let egl = enumerate_egl_vendor_jsons();

    let mut result: Vec<DetectedGpu> = physical
        .into_iter()
        .map(|p| {
            let egl_path = match p.vendor.as_str() {
                "NVIDIA" => egl.nvidia.clone(),
                // Intel + AMD + Nouveau all ship through Mesa EGL.
                _ => egl.mesa.clone(),
            };
            DetectedGpu {
                id: p.pci_slot.clone(),
                vendor: p.vendor.clone(),
                name: p.name.clone(),
                pci_id: Some(format!("{}:{}", p.pci_vendor_hex, p.pci_device_hex)),
                kind: p.kind,
                is_usable: egl_path.is_some(),
                egl_vendor_json: egl_path,
            }
        })
        .collect();

    // Always expose Software as a fallback. Mesa ships llvmpipe through
    // the same JSON the user can pin EGL to via LIBGL_ALWAYS_SOFTWARE=1;
    // the runtime branch in main.rs handles the env wiring.
    result.push(DetectedGpu {
        id: "software".to_string(),
        vendor: "Software".to_string(),
        name: "Software (CPU, llvmpipe)".to_string(),
        pci_id: None,
        kind: GpuKind::Software,
        egl_vendor_json: egl.mesa.clone(),
        is_usable: egl.mesa.is_some(),
    });

    result
}

// ---- Physical PCI detection ------------------------------------------------

#[derive(Debug, Clone)]
struct PhysicalGpu {
    pci_slot: String,
    pci_vendor_hex: String,
    pci_device_hex: String,
    vendor: String,
    name: String,
    kind: GpuKind,
}

#[cfg(not(target_os = "linux"))]
fn detect_physical_gpus() -> Vec<PhysicalGpu> {
    // macOS / Windows: no sysfs path to enumerate from. The picker will
    // only show the Software fallback there, which is the right shape
    // until we add platform-specific detection.
    Vec::new()
}

#[cfg(not(target_os = "linux"))]
fn enumerate_egl_vendor_jsons() -> EglVendors {
    EglVendors::default()
}

#[cfg(target_os = "linux")]
fn detect_physical_gpus() -> Vec<PhysicalGpu> {
    let drm_root = Path::new("/sys/class/drm");
    let entries = match fs::read_dir(drm_root) {
        Ok(it) => it,
        Err(_) => return Vec::new(),
    };

    let mut gpus: Vec<PhysicalGpu> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Match `card0`, `card1`, ... — skip the `card0-DP-1` connector
        // pseudo-entries (have a dash in the name) and the renderD*
        // nodes (different prefix).
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device_path = entry.path().join("device");
        let canonical = match fs::canonicalize(&device_path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let pci_slot = pci_slot_from_path(&canonical).unwrap_or_else(|| name.clone());

        let pci_vendor_hex = read_trimmed(&device_path.join("vendor")).unwrap_or_default();
        let pci_device_hex = read_trimmed(&device_path.join("device")).unwrap_or_default();
        let vendor = classify_vendor(&pci_vendor_hex);
        if vendor == "Unknown" {
            // Not a known consumer GPU vendor — skip rather than expose
            // server / virtual GPU rows the user can't really act on.
            continue;
        }

        let product_name = read_trimmed(&device_path.join("product_name"))
            .filter(|s| !s.is_empty());
        let name = build_human_name(&vendor, product_name.as_deref());

        gpus.push(PhysicalGpu {
            pci_slot,
            pci_vendor_hex: strip_hex_prefix(&pci_vendor_hex),
            pci_device_hex: strip_hex_prefix(&pci_device_hex),
            vendor,
            name,
            // Kind needs the full list to compare slots → fill in below.
            kind: GpuKind::Discrete,
        });
    }

    classify_kinds(&mut gpus);
    gpus
}

#[cfg(target_os = "linux")]
fn pci_slot_from_path(path: &Path) -> Option<String> {
    // /sys/devices/pci0000:00/0000:00:01.0/0000:01:00.0 → "0000:01:00.0"
    path.file_name().map(|s| s.to_string_lossy().to_string())
}

#[cfg(target_os = "linux")]
fn read_trimmed(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn strip_hex_prefix(s: &str) -> String {
    s.strip_prefix("0x").unwrap_or(s).to_string()
}

fn classify_vendor(pci_vendor: &str) -> String {
    match pci_vendor {
        VENDOR_INTEL => "Intel".to_string(),
        VENDOR_AMD => "AMD".to_string(),
        VENDOR_NVIDIA => "NVIDIA".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn build_human_name(vendor: &str, product: Option<&str>) -> String {
    match (vendor, product) {
        (_, Some(p)) if !p.is_empty() => p.to_string(),
        ("Intel", None) => "Intel Graphics".to_string(),
        ("AMD", None) => "AMD Graphics".to_string(),
        ("NVIDIA", None) => "NVIDIA Graphics".to_string(),
        _ => "GPU".to_string(),
    }
}

/// Classify each GPU as Integrated vs Discrete using a small heuristic
/// over the full list:
///   - NVIDIA is always Discrete (consumer NVIDIA iGPUs don't exist).
///   - Intel: if there is also a NVIDIA/AMD discrete present, Intel is
///     Integrated. If Intel is alone, infer from PCI slot — slot
///     `0000:00:...` (root bus) is integrated, anything beyond is
///     treated as discrete (covers Intel Arc desktop cards).
///   - AMD: tricky. If there is also a NVIDIA, AMD is the iGPU (APU).
///     If AMD is alone, infer from slot like Intel.
fn classify_kinds(gpus: &mut [PhysicalGpu]) {
    let has_nvidia = gpus.iter().any(|g| g.vendor == "NVIDIA");
    let has_amd = gpus.iter().any(|g| g.vendor == "AMD");

    for g in gpus.iter_mut() {
        g.kind = match g.vendor.as_str() {
            "NVIDIA" => GpuKind::Discrete,
            "Intel" => {
                if has_nvidia || has_amd {
                    GpuKind::Integrated
                } else if slot_is_root_bus(&g.pci_slot) {
                    GpuKind::Integrated
                } else {
                    GpuKind::Discrete
                }
            }
            "AMD" => {
                if has_nvidia {
                    GpuKind::Integrated
                } else if slot_is_root_bus(&g.pci_slot) {
                    GpuKind::Integrated
                } else {
                    GpuKind::Discrete
                }
            }
            _ => GpuKind::Discrete,
        };
    }
}

fn slot_is_root_bus(slot: &str) -> bool {
    // PCI slot format: "DDDD:BB:DD.F" where BB is the bus. Root bus is 00.
    slot.split(':').nth(1).map(|bb| bb == "00").unwrap_or(false)
}

// ---- EGL vendor JSON enumeration ------------------------------------------

#[derive(Debug, Clone, Default)]
struct EglVendors {
    mesa: Option<String>,
    nvidia: Option<String>,
}

#[cfg(target_os = "linux")]
fn enumerate_egl_vendor_jsons() -> EglVendors {
    let candidates: [&str; 2] = [
        "/usr/share/glvnd/egl_vendor.d",
        "/etc/glvnd/egl_vendor.d",
    ];
    let mut out = EglVendors::default();
    for root in candidates {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !file_name.ends_with(".json") {
                continue;
            }
            let lower = file_name.to_lowercase();
            let abs = path.to_string_lossy().to_string();
            if lower.contains("nvidia") {
                if out.nvidia.is_none() {
                    out.nvidia = Some(abs);
                }
            } else if lower.contains("mesa") {
                if out.mesa.is_none() {
                    out.mesa = Some(abs);
                }
            }
        }
    }
    out
}

/// An action to apply to the process environment. We need `Unset` (not
/// just `Set`) because the parent shell/session may have already set
/// vars that pull rendering back to the wrong GPU — most importantly
/// `__NV_PRIME_RENDER_OFFLOAD=1` on Optimus laptops, which makes NVIDIA
/// hijack the render even after we pin EGL to Mesa.
#[derive(Debug, Clone)]
pub enum EnvAction {
    Set(String, String),
    Unset(String),
}

/// Decide what `PreferredGpu::Auto` should pin on this system. The
/// rule: if we detect both an iGPU and a dGPU that are usable, pin to
/// the iGPU — that's the sweet-spot config for hybrid laptops (Optimus,
/// Mux-less Ryzen, etc.) where the dGPU otherwise hijacks WebKit
/// rendering and wastes battery. Single-GPU or no-GPU systems return
/// None → caller leaves env alone and the system default wins (correct
/// behavior when there's nothing to choose between).
pub fn auto_resolves_to<'a>(detected: &'a [DetectedGpu]) -> Option<&'a DetectedGpu> {
    let integrated = detected
        .iter()
        .find(|g| g.kind == GpuKind::Integrated && g.is_usable);
    let has_discrete = detected
        .iter()
        .any(|g| g.kind == GpuKind::Discrete && g.is_usable);
    if integrated.is_some() && has_discrete {
        integrated
    } else {
        None
    }
}

/// Build the env actions that pin the running process to a specific GPU
/// stack. The caller (main.rs at startup) applies them via
/// `std::env::set_var` / `remove_var` before WebKit initializes.
/// Returning an empty vec means "auto with no resolution" — don't touch
/// the environment.
pub fn env_vars_for_preferred_gpu(
    preferred: &PreferredGpu,
    detected: &[DetectedGpu],
) -> Vec<EnvAction> {
    match preferred {
        PreferredGpu::Auto => match auto_resolves_to(detected) {
            Some(g) => env_vars_for_gpu(g),
            None => Vec::new(),
        },
        PreferredGpu::Software => {
            // Drop to software rendering across every layer that can
            // sneak the GPU back in. Same set as scripts/dev-cpu-mode.sh.
            vec![
                EnvAction::Set("LIBGL_ALWAYS_SOFTWARE".to_string(), "1".to_string()),
                EnvAction::Set("WEBKIT_DISABLE_DMABUF_RENDERER".to_string(), "1".to_string()),
                EnvAction::Set("GSK_RENDERER".to_string(), "cairo".to_string()),
                EnvAction::Unset("__NV_PRIME_RENDER_OFFLOAD".to_string()),
            ]
        }
        PreferredGpu::Kind(kind) => {
            let target = detected
                .iter()
                .find(|g| g.kind == *kind && g.is_usable);
            let Some(g) = target else {
                return Vec::new();
            };
            env_vars_for_gpu(g)
        }
        PreferredGpu::SpecificId(id) => {
            let target = detected.iter().find(|g| &g.id == id);
            let Some(g) = target else {
                return Vec::new();
            };
            env_vars_for_gpu(g)
        }
    }
}

fn env_vars_for_gpu(g: &DetectedGpu) -> Vec<EnvAction> {
    let mut env: Vec<EnvAction> = Vec::new();
    if let Some(path) = &g.egl_vendor_json {
        env.push(EnvAction::Set(
            "__EGL_VENDOR_LIBRARY_FILENAMES".to_string(),
            path.clone(),
        ));
    }
    match g.vendor.as_str() {
        "Intel" | "AMD" => {
            env.push(EnvAction::Set(
                "__GLX_VENDOR_LIBRARY_NAME".to_string(),
                "mesa".to_string(),
            ));
            env.push(EnvAction::Set("DRI_PRIME".to_string(), "0".to_string()));
            // Kill any NVIDIA PRIME offload the session might have set.
            // Without this, on Optimus laptops the dGPU still wins even
            // with EGL/GLX pinned to Mesa — `__NV_PRIME_RENDER_OFFLOAD=1`
            // takes priority and routes WebKit composition back to
            // NVIDIA. Mirrors `unset __NV_PRIME_RENDER_OFFLOAD` in
            // scripts/dev-igpu-mode.sh.
            env.push(EnvAction::Unset(
                "__NV_PRIME_RENDER_OFFLOAD".to_string(),
            ));
        }
        "NVIDIA" => {
            env.push(EnvAction::Set(
                "__NV_PRIME_RENDER_OFFLOAD".to_string(),
                "1".to_string(),
            ));
            env.push(EnvAction::Set(
                "__GLX_VENDOR_LIBRARY_NAME".to_string(),
                "nvidia".to_string(),
            ));
        }
        _ => {}
    }
    env
}

/// User-facing preference. Mirrors the column in graphics_settings.db
/// (free-form text with a small known set of values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferredGpu {
    Auto,
    Software,
    Kind(GpuKind),
    SpecificId(String),
}

impl PreferredGpu {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "" | "auto" => Self::Auto,
            "software" | "cpu" => Self::Software,
            "integrated" | "igpu" => Self::Kind(GpuKind::Integrated),
            "discrete" | "dgpu" => Self::Kind(GpuKind::Discrete),
            other => Self::SpecificId(other.to_string()),
        }
    }

    pub fn as_storage_string(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Software => "software".to_string(),
            Self::Kind(GpuKind::Integrated) => "integrated".to_string(),
            Self::Kind(GpuKind::Discrete) => "discrete".to_string(),
            Self::Kind(GpuKind::Software) => "software".to_string(),
            Self::SpecificId(id) => id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_gpu_round_trip() {
        for raw in ["auto", "software", "integrated", "discrete", "0000:01:00.0"] {
            let parsed = PreferredGpu::parse(raw);
            assert_eq!(parsed.as_storage_string(), raw);
        }
    }

    #[test]
    fn slot_root_bus_classification() {
        assert!(slot_is_root_bus("0000:00:02.0"));
        assert!(slot_is_root_bus("0000:00:01.0"));
        assert!(!slot_is_root_bus("0000:01:00.0"));
        assert!(!slot_is_root_bus("0000:02:00.0"));
    }

    #[test]
    fn vendor_classification() {
        assert_eq!(classify_vendor(VENDOR_INTEL), "Intel");
        assert_eq!(classify_vendor(VENDOR_AMD), "AMD");
        assert_eq!(classify_vendor(VENDOR_NVIDIA), "NVIDIA");
        assert_eq!(classify_vendor("0xdeadbeef"), "Unknown");
    }
}
