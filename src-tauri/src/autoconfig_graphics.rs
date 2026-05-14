//! Graphics auto-configuration tool
//!
//! Detects the current GPU, display server, desktop environment, and compositor,
//! then recommends and applies optimal graphics settings.
//!
//! Two entry points:
//!   - `run()` — CLI prompt invoked via `qbz --autoconfig-graphics`
//!   - `detect_environment()` / `compute_recommendation()` / `apply_recommendation()`
//!     — public helpers consumed by V2 commands that surface the recommendation
//!     inside the Settings UI (issue #315 follow-up)

use crate::config::graphics_settings::GraphicsSettingsStore;
use serde::Serialize;
use std::io::{self, BufRead, Write};

/// Detected environment information
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Environment {
    pub display_server: String,
    pub gpu_nvidia: bool,
    pub gpu_amd: bool,
    pub gpu_intel: bool,
    pub gpu_name: String,
    pub desktop: String,
    pub is_vm: bool,
}

/// Recommended configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Recommendation {
    pub hardware_acceleration: bool,
    pub force_x11: bool,
    pub gsk_renderer: Option<String>,
    pub disable_dmabuf: bool,
    pub disable_blur_background: bool,
    pub rationale: Vec<String>,
}

/// Run the autoconfig-graphics CLI tool.
pub fn run() {
    eprintln!("[QBZ AutoConfig] Detecting environment...");
    eprintln!();

    let env = detect_environment();
    print_environment(&env);

    let rec = compute_recommendation(&env);
    print_recommendation(&rec);

    eprintln!();
    eprint!("Apply this configuration? [Y/n] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    if io::stdin().lock().read_line(&mut input).is_ok() {
        let answer = input.trim().to_lowercase();
        if answer.is_empty() || answer == "y" || answer == "yes" {
            apply_recommendation(&rec);
        } else {
            eprintln!("[QBZ AutoConfig] Aborted. No changes made.");
        }
    } else {
        eprintln!("[QBZ AutoConfig] Could not read input. No changes made.");
    }
}

pub fn detect_environment() -> Environment {
    let display_server = detect_display_server();
    let gpu_nvidia = is_nvidia_gpu();
    let gpu_amd = is_amd_gpu();
    let gpu_intel = is_intel_gpu();
    let gpu_name = detect_gpu_name(gpu_nvidia, gpu_amd, gpu_intel);
    let desktop = detect_desktop();
    let is_vm = is_virtual_machine();

    Environment {
        display_server,
        gpu_nvidia,
        gpu_amd,
        gpu_intel,
        gpu_name,
        desktop,
        is_vm,
    }
}

fn detect_display_server() -> String {
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland");

    if is_wayland {
        "Wayland".to_string()
    } else {
        "X11".to_string()
    }
}

fn is_nvidia_gpu() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
        || std::fs::read_to_string("/proc/modules")
            .map(|m| m.lines().any(|l| l.starts_with("nvidia")))
            .unwrap_or(false)
}

fn is_amd_gpu() -> bool {
    std::path::Path::new("/sys/module/amdgpu").exists()
        || std::fs::read_to_string("/proc/modules")
            .map(|m| m.lines().any(|l| l.starts_with("amdgpu")))
            .unwrap_or(false)
}

fn is_intel_gpu() -> bool {
    std::path::Path::new("/sys/module/i915").exists()
        || std::fs::read_to_string("/proc/modules")
            .map(|m| m.lines().any(|l| l.starts_with("i915")))
            .unwrap_or(false)
}

fn is_virtual_machine() -> bool {
    if let Ok(product) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
        let p = product.trim().to_lowercase();
        if p.contains("virtualbox")
            || p.contains("vmware")
            || p.contains("qemu")
            || p.contains("bochs")
            || p.contains("hyper-v")
        {
            return true;
        }
    }
    if let Ok(vendor) = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
        let v = vendor.trim().to_lowercase();
        if v.contains("innotek")
            || v.contains("vmware")
            || v.contains("qemu")
            || v.contains("xen")
            || v.contains("parallels")
        {
            return true;
        }
    }
    if let Ok(h) = std::fs::read_to_string("/sys/hypervisor/type") {
        if !h.trim().is_empty() {
            return true;
        }
    }
    false
}

pub fn detect_gpu_name(nvidia: bool, amd: bool, intel: bool) -> String {
    // Hybrid laptops have more than one of these set; join the names
    // so diagnostics surface the full picture instead of returning only
    // the first vendor matched.
    let mut parts: Vec<String> = Vec::new();
    if nvidia {
        parts.push(nvidia_name());
    }
    if amd {
        parts.push(amd_name());
    }
    if intel {
        parts.push(intel_name());
    }
    if parts.is_empty() {
        "Unknown / None detected".to_string()
    } else {
        parts.join(" + ")
    }
}

fn nvidia_name() -> String {
    if let Ok(version) = std::fs::read_to_string("/proc/driver/nvidia/version") {
        if let Some(line) = version.lines().next() {
            return format!("NVIDIA ({})", line.trim());
        }
    }
    "NVIDIA (driver loaded)".to_string()
}

fn amd_name() -> String {
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("card") && !name.contains('-') {
                let model_path = entry.path().join("device/product_name");
                if let Ok(model) = std::fs::read_to_string(&model_path) {
                    let model = model.trim();
                    if !model.is_empty() {
                        return format!("AMD {}", model);
                    }
                }
            }
        }
    }
    "AMD (amdgpu driver loaded)".to_string()
}

fn intel_name() -> String {
    "Intel (i915/xe driver loaded)".to_string()
}

fn detect_desktop() -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = std::env::var("XDG_SESSION_DESKTOP").unwrap_or_default();
    let de = std::env::var("DESKTOP_SESSION").unwrap_or_default();

    if !desktop.is_empty() {
        desktop
    } else if !session.is_empty() {
        session
    } else if !de.is_empty() {
        de
    } else {
        "Unknown".to_string()
    }
}

fn print_environment(env: &Environment) {
    eprintln!("  Display server : {}", env.display_server);
    eprintln!("  GPU            : {}", env.gpu_name);
    eprintln!("  Desktop        : {}", env.desktop);
    if env.is_vm {
        eprintln!("  Virtual machine: Yes");
    }
    eprintln!();
}

pub fn compute_recommendation(env: &Environment) -> Recommendation {
    let mut rationale = Vec::new();

    // VM: software rendering, no blur
    if env.is_vm {
        rationale.push("Virtual machine detected: using software rendering".to_string());
        return Recommendation {
            hardware_acceleration: false,
            force_x11: false,
            gsk_renderer: Some("cairo".to_string()),
            disable_dmabuf: true,
            disable_blur_background: true,
            rationale,
        };
    }

    let is_wayland = env.display_server == "Wayland";
    let desktop_lower = env.desktop.to_lowercase();
    let is_gnome = desktop_lower.contains("gnome");
    let has_hybrid_igpu = env.gpu_nvidia && (env.gpu_intel || env.gpu_amd);

    // Hybrid laptops (NVIDIA dGPU + Intel/AMD iGPU). WebKit composes via
    // the iGPU through EGL/GLX defaults — the NVIDIA card sits idle for
    // PRIME render offload. Forcing GSK_RENDERER=gl here was hurting
    // performance because the GL renderer biases toward the dGPU stack
    // even when the iGPU is the actual paint target. Auto (None) lets
    // GTK4 pick NGL/Vulkan as appropriate for the iGPU. DMA-BUF stays
    // disabled — that one is fragile on any NVIDIA-touching setup.
    if has_hybrid_igpu {
        let igpu_label = if env.gpu_intel { "Intel" } else { "AMD" };
        rationale.push(format!(
            "NVIDIA + {} hybrid: iGPU handles WebKit compositing, leaving GSK at Auto",
            igpu_label
        ));
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: None,
            disable_dmabuf: true,
            disable_blur_background: false,
            rationale,
        };
    }

    // NVIDIA + Wayland + GNOME: known stutter combo
    if env.gpu_nvidia && is_wayland && is_gnome {
        rationale.push("NVIDIA + Wayland + GNOME: using GL renderer, DMA-BUF off".to_string());
        rationale.push("This combination has known compositing issues".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: Some("gl".to_string()),
            disable_dmabuf: true,
            disable_blur_background: false,
            rationale,
        };
    }

    // NVIDIA + Wayland (non-GNOME)
    if env.gpu_nvidia && is_wayland {
        rationale.push("NVIDIA + Wayland: using GL renderer, DMA-BUF off".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: Some("gl".to_string()),
            disable_dmabuf: true,
            disable_blur_background: false,
            rationale,
        };
    }

    // NVIDIA + X11
    if env.gpu_nvidia {
        rationale.push("NVIDIA + X11: full hardware acceleration, DMA-BUF off".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: Some("gl".to_string()),
            disable_dmabuf: true,
            disable_blur_background: false,
            rationale,
        };
    }

    // AMD + Wayland
    if env.gpu_amd && is_wayland {
        rationale.push("AMD + Wayland: NGL renderer with DMA-BUF".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: Some("ngl".to_string()),
            disable_dmabuf: false,
            disable_blur_background: false,
            rationale,
        };
    }

    // AMD + X11
    if env.gpu_amd {
        rationale.push("AMD + X11: full hardware acceleration".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: None,
            disable_dmabuf: false,
            disable_blur_background: false,
            rationale,
        };
    }

    // Intel + Wayland
    if env.gpu_intel && is_wayland {
        rationale.push("Intel + Wayland: NGL renderer with DMA-BUF".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: Some("ngl".to_string()),
            disable_dmabuf: false,
            disable_blur_background: false,
            rationale,
        };
    }

    // Intel + X11
    if env.gpu_intel {
        rationale.push("Intel + X11: full hardware acceleration".to_string());
        return Recommendation {
            hardware_acceleration: true,
            force_x11: false,
            gsk_renderer: None,
            disable_dmabuf: false,
            disable_blur_background: false,
            rationale,
        };
    }

    // Unknown GPU: safe defaults
    rationale.push("No known GPU detected: using safe defaults".to_string());
    Recommendation {
        hardware_acceleration: true,
        force_x11: false,
        gsk_renderer: None,
        disable_dmabuf: false,
        disable_blur_background: false,
        rationale,
    }
}

fn print_recommendation(rec: &Recommendation) {
    eprintln!("[QBZ AutoConfig] Recommended configuration:");
    eprintln!(
        "  hardware_acceleration  : {}",
        if rec.hardware_acceleration {
            "on"
        } else {
            "off"
        }
    );
    eprintln!(
        "  force_x11              : {}",
        if rec.force_x11 { "on" } else { "off" }
    );
    eprintln!(
        "  gsk_renderer           : {}",
        rec.gsk_renderer.as_deref().unwrap_or("auto")
    );
    eprintln!(
        "  disable_dmabuf         : {}",
        if rec.disable_dmabuf { "yes" } else { "no" }
    );
    eprintln!(
        "  disable_blur_background: {}",
        if rec.disable_blur_background {
            "yes"
        } else {
            "no"
        }
    );
    eprintln!();
    for reason in &rec.rationale {
        eprintln!("  Rationale: {}", reason);
    }
}

fn apply_recommendation(rec: &Recommendation) {
    match write_recommendation(rec) {
        Ok(()) => {
            if rec.disable_blur_background {
                eprintln!(
                    "[QBZ AutoConfig] Note: blur background will be disabled. You can toggle this in Settings > Appearance."
                );
            }
            eprintln!();
            eprintln!("[QBZ AutoConfig] Configuration applied successfully.");
            eprintln!("[QBZ AutoConfig] Restart QBZ to take effect.");
        }
        Err(errors) => {
            eprintln!();
            eprintln!("[QBZ AutoConfig] Some settings could not be applied:");
            for e in &errors {
                eprintln!("  - {}", e);
            }
        }
    }
}

/// Apply the recommendation to the persistence layer. Shared between the CLI
/// prompt (`apply_recommendation`) and the V2 command surface. Returns the
/// list of write errors so the caller can surface them appropriately (stderr
/// for CLI, frontend toast for the Settings UI).
///
/// DMA-BUF semantics after the 1.2.13 opt-in flip:
///   - `rec.disable_dmabuf = true`  → force_dmabuf = false (matches default;
///     runtime keeps DMA-BUF off).
///   - `rec.disable_dmabuf = false` → force_dmabuf = true  (user opts in via
///     this recommendation; runtime turns DMA-BUF on).
pub fn write_recommendation(rec: &Recommendation) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    match GraphicsSettingsStore::new() {
        Ok(store) => {
            if let Err(e) = store.set_hardware_acceleration(rec.hardware_acceleration) {
                errors.push(format!("hardware_acceleration: {}", e));
            }
            if let Err(e) = store.set_force_x11(rec.force_x11) {
                errors.push(format!("force_x11: {}", e));
            }
            if let Err(e) = store.set_gsk_renderer(rec.gsk_renderer.clone()) {
                errors.push(format!("gsk_renderer: {}", e));
            }
        }
        Err(e) => {
            errors.push(format!("graphics settings store: {}", e));
        }
    }

    let desired_force_dmabuf = !rec.disable_dmabuf;
    match crate::config::developer_settings::DeveloperSettingsStore::new() {
        Ok(store) => {
            if let Err(e) = store.set_force_dmabuf(desired_force_dmabuf) {
                errors.push(format!("force_dmabuf: {}", e));
            }
        }
        Err(e) => {
            errors.push(format!("developer settings store: {}", e));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
