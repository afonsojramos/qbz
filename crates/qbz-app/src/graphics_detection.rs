//! Host GPU enumeration for the current desktop shell.
//!
//! Cross-references the PCI devices visible under `/sys/class/drm/` (real
//! physical GPUs) with the EGL vendor JSONs available under
//! `/usr/share/glvnd/egl_vendor.d/` (stacks the running process can
//! actually use). The intersection is what the host shell can expose to
//! the user.
//!
//! Inside Flatpak/Snap the `/sys` path is still mounted (read-only) so
//! PCI detection works; the `/usr/share/glvnd/...` view reflects the
//! sandbox's own vendor JSONs which is the correct thing to read.
//!
//! Companion to graphics auto-configuration logic, which decides "what
//! config should we recommend". This module answers "what hardware/stacks
//! do we actually have".
//!
//! Cross-platform contract: the public API (`enumerate_gpus`,
//! `env_vars_for_preferred_gpu`, `PreferredGpu`) compiles on every
//! platform. macOS/Windows fall through to empty enumerations and a
//! no-op env-var application; the Software entry still round-trips
//! through storage without breaking the build.

use serde::Serialize;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;

/// PCI vendor IDs we recognize.
const VENDOR_INTEL: &str = "0x8086";
const VENDOR_AMD: &str = "0x1002";
const VENDOR_NVIDIA: &str = "0x10de";

/// Coarse classification of a GPU's role on the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuKind {
    Integrated,
    Discrete,
    /// Pure-software rendering (llvmpipe / cpu fallback). Not a real device.
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
    /// Human-readable model where available.
    pub name: String,
    /// PCI ID as `vendor:device` for diagnostics.
    pub pci_id: Option<String>,
    pub kind: GpuKind,
    /// Absolute path of the EGL vendor JSON that exposes this GPU's stack to
    /// the running process, if one is available.
    pub egl_vendor_json: Option<String>,
    /// True when the GPU is both physically present and reachable via an EGL
    /// vendor JSON.
    pub is_usable: bool,
}

/// Enumerate every GPU the user could pick. Always returns at least the
/// Software entry so the dropdown is never empty on systems with no detectable
/// hardware.
pub fn enumerate_gpus() -> Vec<DetectedGpu> {
    let physical = detect_physical_gpus();
    let egl = enumerate_egl_vendor_jsons();

    let mut result: Vec<DetectedGpu> = physical
        .into_iter()
        .map(|p| {
            let egl_path = match p.vendor.as_str() {
                "NVIDIA" => egl.nvidia.clone(),
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
            continue;
        }

        let product_name =
            read_trimmed(&device_path.join("product_name")).filter(|s| !s.is_empty());
        let name = build_human_name(&vendor, product_name.as_deref());

        gpus.push(PhysicalGpu {
            pci_slot,
            pci_vendor_hex: strip_hex_prefix(&pci_vendor_hex),
            pci_device_hex: strip_hex_prefix(&pci_device_hex),
            vendor,
            name,
            kind: GpuKind::Discrete,
        });
    }

    classify_kinds(&mut gpus);
    gpus
}

#[cfg(target_os = "linux")]
fn pci_slot_from_path(path: &Path) -> Option<String> {
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

fn classify_kinds(gpus: &mut [PhysicalGpu]) {
    let has_nvidia = gpus.iter().any(|g| g.vendor == "NVIDIA");
    let has_amd = gpus.iter().any(|g| g.vendor == "AMD");

    for g in gpus.iter_mut() {
        g.kind = match g.vendor.as_str() {
            "NVIDIA" => GpuKind::Discrete,
            "Intel" => {
                if has_nvidia || has_amd || slot_is_root_bus(&g.pci_slot) {
                    GpuKind::Integrated
                } else {
                    GpuKind::Discrete
                }
            }
            "AMD" => {
                if has_nvidia || slot_is_root_bus(&g.pci_slot) {
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
    slot.split(':').nth(1).map(|bb| bb == "00").unwrap_or(false)
}

#[derive(Debug, Clone, Default)]
struct EglVendors {
    mesa: Option<String>,
    nvidia: Option<String>,
}

#[cfg(target_os = "linux")]
fn enumerate_egl_vendor_jsons() -> EglVendors {
    let candidates: [&str; 2] = ["/usr/share/glvnd/egl_vendor.d", "/etc/glvnd/egl_vendor.d"];
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
            } else if lower.contains("mesa") && out.mesa.is_none() {
                out.mesa = Some(abs);
            }
        }
    }
    out
}

/// An action for the host shell to apply to the process environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvAction {
    Set(String, String),
    Unset(String),
}

/// Decide what `PreferredGpu::Auto` should pin on this system.
pub fn auto_resolves_to(detected: &[DetectedGpu]) -> Option<&DetectedGpu> {
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

/// Build the env actions that pin the running process to a specific GPU stack.
/// Returning an empty vec means "auto with no resolution".
pub fn env_vars_for_preferred_gpu(
    preferred: &PreferredGpu,
    detected: &[DetectedGpu],
) -> Vec<EnvAction> {
    match preferred {
        PreferredGpu::Auto => match auto_resolves_to(detected) {
            Some(g) => env_vars_for_gpu(g),
            None => Vec::new(),
        },
        PreferredGpu::Software => vec![
            EnvAction::Set("LIBGL_ALWAYS_SOFTWARE".to_string(), "1".to_string()),
            EnvAction::Set(
                "WEBKIT_DISABLE_DMABUF_RENDERER".to_string(),
                "1".to_string(),
            ),
            EnvAction::Set("GSK_RENDERER".to_string(), "cairo".to_string()),
            EnvAction::Unset("__NV_PRIME_RENDER_OFFLOAD".to_string()),
        ],
        PreferredGpu::Kind(kind) => {
            let target = detected.iter().find(|g| g.kind == *kind && g.is_usable);
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
            env.push(EnvAction::Unset("__NV_PRIME_RENDER_OFFLOAD".to_string()));
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

/// User-facing preference. Mirrors the column in graphics_settings.db.
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

    fn detected_gpu(
        id: &str,
        vendor: &str,
        kind: GpuKind,
        is_usable: bool,
        egl_vendor_json: Option<&str>,
    ) -> DetectedGpu {
        DetectedGpu {
            id: id.to_string(),
            vendor: vendor.to_string(),
            name: format!("{vendor} Graphics"),
            pci_id: None,
            kind,
            egl_vendor_json: egl_vendor_json.map(str::to_string),
            is_usable,
        }
    }

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

    #[test]
    fn auto_resolves_to_integrated_gpu_on_hybrid_system() {
        let gpus = vec![
            detected_gpu(
                "0000:00:02.0",
                "Intel",
                GpuKind::Integrated,
                true,
                Some("/mesa.json"),
            ),
            detected_gpu(
                "0000:01:00.0",
                "NVIDIA",
                GpuKind::Discrete,
                true,
                Some("/nvidia.json"),
            ),
        ];

        let resolved = auto_resolves_to(&gpus).expect("hybrid should resolve");

        assert_eq!(resolved.id, "0000:00:02.0");
    }

    #[test]
    fn auto_does_not_resolve_without_usable_discrete_gpu() {
        let gpus = vec![
            detected_gpu(
                "0000:00:02.0",
                "Intel",
                GpuKind::Integrated,
                true,
                Some("/mesa.json"),
            ),
            detected_gpu(
                "0000:01:00.0",
                "NVIDIA",
                GpuKind::Discrete,
                false,
                Some("/nvidia.json"),
            ),
        ];

        assert!(auto_resolves_to(&gpus).is_none());
    }

    #[test]
    fn software_preference_returns_cpu_env_actions() {
        let actions = env_vars_for_preferred_gpu(&PreferredGpu::Software, &[]);

        assert!(actions.contains(&EnvAction::Set(
            "LIBGL_ALWAYS_SOFTWARE".to_string(),
            "1".to_string()
        )));
        assert!(actions.contains(&EnvAction::Set(
            "WEBKIT_DISABLE_DMABUF_RENDERER".to_string(),
            "1".to_string()
        )));
        assert!(actions.contains(&EnvAction::Set(
            "GSK_RENDERER".to_string(),
            "cairo".to_string()
        )));
        assert!(actions.contains(&EnvAction::Unset("__NV_PRIME_RENDER_OFFLOAD".to_string())));
    }
}
