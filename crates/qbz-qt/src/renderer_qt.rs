//! The renderer TIER — one source of truth for "what is actually drawing, and
//! what may we offer because of it".
//!
//! # The gap this closes
//!
//! Slint resolves `use_gpu_renderer` from `select_slint_backend()` and hands
//! that ONE bool to three features (`crates/qbz/src/main.rs`):
//!
//! | consumer | line | meaning |
//! |---|---|---|
//! | `shader_scenes_available` | `:8557` | the 6 immersive shader scenes — they render BLACK off the GPU tier |
//! | `app_background_available` | `:8563` | the app-wide dynamic background |
//! | `reduce_motion` | `:8597` | `kiosk_profile \|\| !use_gpu_renderer` |
//!
//! This port had none of it. `reduce_motion` shipped as the kiosk half alone
//! (`shell_bridge::reduce_motion_at_boot`), and the immersive contract records
//! the rest as deliberately absent (D11: *"The RENDERER-TIER half is still
//! genuinely absent"*) — which is precisely why the immersive shader scenes
//! could not be offered: there was nothing to gate them on.
//!
//! # Why the truth can only come from QML
//!
//! `main::apply_renderer_preference()` runs BEFORE `QGuiApplication` and knows
//! only what we ASKED FOR. What we actually GOT is decided by QRhi when the
//! scene graph initialises, and is readable only as `GraphicsInfo.api` on a
//! live item. Asking for OpenGL on a box whose driver refuses it lands on
//! software, and nothing before the window exists can know that.
//!
//! So QML reports the resolved api in exactly one place (`Main.qml`'s renderer
//! probe) and this module latches it. Slint has the same split; its
//! `select_slint_backend()` actually initialises the backend, which is why it
//! can return the truth synchronously and we cannot.
//!
//! # Two questions, two answers — do NOT collapse them
//!
//! * *"Can THIS item draw a shader right now?"* → the item's own
//!   `GraphicsInfo.api`, which is per-window and already correct at the six
//!   call sites that use it (`RoundedImage`, `Cortinilla`, `PlaylistCollage`,
//!   `TrackInfoModal`, `ImmersiveView`, `StaticPanel`).
//! * *"Should this FEATURE be offered at all?"* → this tier.
//!
//! They look like duplicates and are not: the first is a drawing decision on
//! one item, the second is a product decision made once. Collapsing the first
//! onto this module would also make every cover in the app wait for a
//! round trip through Rust before it could pick its draw path.
//!
//! # The default is `true`, on purpose
//!
//! Until the probe reports, the tier reads GPU-capable. Both platforms were
//! measured on the GPU (OpenGL RHI on Linux, Metal on macOS, 2026-07-29), so
//! `true` is the honest prior AND it keeps today's behaviour byte for byte
//! until the probe disagrees — a `false` default would step every animation to
//! the reduce-motion cadence for the first frames of every launch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The resolved `GraphicsInfo.api`, lowercased: "opengl" | "metal" | "vulkan"
/// | "d3d11" | "d3d12" | "software" | "null" | "unknown".
static ACTIVE_API: Mutex<String> = Mutex::new(String::new());

/// Does the active backend carry the GPU tier? See the module header for why
/// this starts `true`.
static GPU_TIER: AtomicBool = AtomicBool::new(true);

/// Has a real (non-"unknown") report landed? Guards the one-shot log and lets
/// callers tell "GPU" from "not asked yet".
static RESOLVED: AtomicBool = AtomicBool::new(false);

/// The two api names that mean "no shaders": Qt's software rasteriser, and the
/// null backend the offscreen platform uses.
fn api_is_gpu(api: &str) -> bool {
    !matches!(api, "software" | "null" | "unknown" | "")
}

/// Latch the api QRhi actually gave us. Called once from the QML probe; a
/// repeat with the same value is free, and "unknown" (the pre-resolution
/// value) is ignored rather than latched as "no GPU".
///
/// Returns true when this call CHANGED the tier, so the caller can decide
/// whether anything needs republishing.
pub fn set_active_api(api: &str) -> bool {
    let api = api.trim().to_ascii_lowercase();
    if api.is_empty() || api == "unknown" {
        return false;
    }
    let tier = api_is_gpu(&api);
    let previous_tier = GPU_TIER.swap(tier, Ordering::SeqCst);
    let first = !RESOLVED.swap(true, Ordering::SeqCst);
    {
        let mut slot = ACTIVE_API.lock().unwrap_or_else(|e| e.into_inner());
        if !first && *slot == api {
            return false;
        }
        *slot = api.clone();
    }
    // Log the REQUESTED tier beside the resolved one: a mismatch is the whole
    // reason this probe exists (asking for opengl on a box whose driver
    // refuses it lands on software), and it is what the crash sentinel needs
    // to reason about.
    log::info!(
        "[qbz-qt] renderer: requested={} resolved={api} gpu_tier={tier}",
        crate::settings_qt::pref_str("renderer", "auto"),
    );
    if !tier {
        log::warn!(
            "[qbz-qt] renderer: NO GPU tier — shader scenes and the dynamic \
             background stay hidden, reduce-motion is forced on"
        );
    }
    first || previous_tier != tier
}

/// True when the active backend can run shaders. The Qt analogue of Slint's
/// `use_gpu_renderer`.
pub fn gpu_tier() -> bool {
    GPU_TIER.load(Ordering::SeqCst)
}

/// The resolved api name, or "unknown" before the probe reports.
pub fn active_api() -> String {
    let slot = ACTIVE_API.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_empty() {
        "unknown".to_string()
    } else {
        slot.clone()
    }
}

/// The reduce-motion value, both halves composed — `kiosk || !gpu_tier`,
/// exactly the reference's expression (`main.rs:8597`).
///
/// It lives here rather than in either half's module because BOTH halves write
/// it: the kiosk live toggle and this probe. When the kiosk toggle owned the
/// property alone it simply overwrote the tier's contribution, which is the
/// bug this function exists to make impossible.
pub fn reduce_motion(kiosk: bool) -> bool {
    kiosk || !gpu_tier()
}

// ===========================================================================
// The startup auto-revert sentinel  (PARITY-DEBT #104's owed half)
// ===========================================================================
//
// Forcing a renderer is the one Settings row that can lock the user OUT of
// Settings: pick a backend this machine cannot start and the next launch dies
// before a window exists, with no way to undo the choice. Slint built a
// sentinel for exactly this (`crates/qbz/src/main.rs:7183-7210`); this port
// shipped the row WITHOUT it, which is what PARITY-DEBT #104 records as still
// owed.
//
// The shape: a file armed BEFORE the risky backend init and cleared once the
// session proves it is alive. Finding it still there at startup means the
// previous forced launch never got that far — so the pref reverts to "auto"
// and this launch uses Qt's own default.
//
// FILENAME: `qt_renderer_attempt`, deliberately NOT Slint's
// `renderer_attempt`. Both apps share `<data>/qbz/`, they can be run in either
// order, and one crashing must never revert the OTHER's renderer choice.

/// `<data>/qbz/qt_renderer_attempt` — see above for why the name differs from
/// the Slint sentinel's.
fn sentinel_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("qt_renderer_attempt"))
}

/// Arm the sentinel for a non-auto choice, just before the env that forces it.
fn arm_sentinel(choice: &str) {
    let Some(path) = sentinel_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, choice) {
        log::warn!("[qbz-qt] renderer: could not arm the startup sentinel: {e}");
    }
}

fn clear_sentinel() {
    if let Some(path) = sentinel_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Is a sentinel currently armed? Lets the liveness report stay quiet on the
/// runs where there is nothing to protect — notably the offscreen smoke, which
/// presents no frames by design and would otherwise log a scary warning on
/// every gate.
pub fn sentinel_armed() -> bool {
    sentinel_path().map(|p| p.exists()).unwrap_or(false)
}

/// The choice the armed sentinel was protecting, if one survived a launch.
fn armed_choice() -> Option<String> {
    std::fs::read_to_string(sentinel_path()?)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Called at the TOP of `apply_renderer_preference()`, before anything is
/// forced. If a previous forced launch left the sentinel armed it never
/// reached liveness, so the choice is reverted to "auto" and this launch runs
/// on Qt's default.
///
/// Returns true when a revert happened — the caller then ignores the persisted
/// pref for this launch (it has just been rewritten to "auto" anyway).
pub fn revert_if_previous_launch_died() -> bool {
    let Some(choice) = armed_choice() else {
        return false;
    };
    clear_sentinel();
    log::warn!(
        "[qbz-qt] renderer: the previous launch forced '{choice}' and never reached \
         liveness — reverting to auto so the choice cannot lock you out of Settings"
    );
    crate::settings_qt::save_pref("renderer", serde_json::json!("auto"));
    true
}

/// Arm for a choice that genuinely FORCES a backend. The caller decides that —
/// the GPU-tier aliases all resolve to Qt's own default and force nothing, so
/// arming for them would let an unrelated crash silently reset the pref.
pub fn arm_for_choice(choice: &str) {
    arm_sentinel(choice);
    log::info!("[qbz-qt] renderer: startup sentinel armed for '{choice}'");
}

/// Drop any sentinel left over from a previous configuration — this launch
/// forces nothing, so there is nothing to protect and nothing to revert.
pub fn clear_stale_sentinel() {
    clear_sentinel();
}

/// Disarm on proof of LIVENESS — frames actually rendered over a window of
/// time, reported by the QML watchdog. Once-guarded, so the repeat calls a
/// binding might make are free.
pub fn disarm_on_liveness(frames: i32) {
    static DISARMED: AtomicBool = AtomicBool::new(false);
    if DISARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    // Only claim a disarm when something WAS armed. Saying "sentinel disarmed"
    // on every ordinary launch trains the reader to skim past the line, which
    // is precisely the run where it carries information.
    if sentinel_armed() {
        log::info!("[qbz-qt] renderer: startup sentinel disarmed ({frames} frames rendered)");
        clear_sentinel();
    } else {
        log::debug!("[qbz-qt] renderer: liveness confirmed ({frames} frames), nothing armed");
    }
}

// ===========================================================================
// Preferred GPU  (PARITY-DEBT #83)
// ===========================================================================
//
// Scoping: `qbz-nix-docs/qt-frontend/2026-08-02-gpu-selection-scoping.md`.
// Qt's own documentation is blunt: QRhi "will choose the system default GPU
// adapter … No further adapter configurability is provided at this time." So
// there is no API — only the driver's env vars, and only on some platforms.
//
// | platform | selectable | how |
// |---|---|---|
// | Linux (OpenGL, this port's default) | YES | `DRI_PRIME`, or the NVIDIA PRIME pair |
// | Windows D3D | yes, when that port lands | `QT_D3D_ADAPTER_INDEX` |
// | macOS Metal | **NO** | QRhi hardcodes `MTLCreateSystemDefaultDevice` |
//
// So: implement Linux, HIDE the row on macOS. A row that cannot do anything is
// worse than no row — and until today this one was worse still, because it
// silently DROPPED every selection that was not "Auto"
// (`settings_qt.rs`, the old `"gpu-power"` arm persisted only index 0).
//
// CLASS KEYS, NEVER NAMES. The pref lives in the ui_prefs.json shared with the
// shipping Slint build, which accepts `"auto" | "integrated" | "discrete"` and
// falls back to Auto for anything it cannot parse. Writing an adapter NAME
// here would silently degrade the Slint side — and its wgpu/Vulkan names would
// not string-match what we read from sysfs anyway.

/// One physical GPU, as Vulkan enumerates it.
///
/// `index` is the position in `vkEnumeratePhysicalDevices` — the SAME number
/// `QT_VK_PHYSICAL_DEVICE_INDEX` selects by, because Qt calls the same
/// function. That identity is the whole reason this row can work at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    pub index: u32,
    /// The model, verbatim from the driver: "NVIDIA GeForce RTX 4070 Laptop
    /// GPU", "Intel(R) Arc(tm) Graphics (MTL)".
    pub name: String,
    pub discrete: bool,
}

impl GpuInfo {
    /// The dropdown label, in the reference's shape
    /// (`"<name> (discrete|integrated)"`, `crates/qbz/src/main.rs:7427-7438`).
    pub fn label(&self) -> String {
        let class = if self.discrete {
            qbz_i18n::t("discrete")
        } else {
            qbz_i18n::t("integrated")
        };
        format!("{} ({})", self.name, class)
    }
}

/// Enumerate the real GPUs, once per process.
///
/// Empty when libvulkan is absent, no ICD is installed, or the instance cannot
/// be created — every one of which means "we cannot honour a GPU choice", so
/// the row collapses to Auto instead of offering hardware that may not exist.
/// That is the defect this replaces: a CLASS dropdown offers "Discrete GPU" on
/// a machine that has none.
pub fn gpus() -> &'static [GpuInfo] {
    static GPUS: std::sync::OnceLock<Vec<GpuInfo>> = std::sync::OnceLock::new();
    GPUS.get_or_init(|| {
        let list = enumerate_vulkan_gpus();
        if list.is_empty() {
            log::info!("[qbz-qt] gpu: no Vulkan devices enumerated — the selector stays on Auto");
        } else {
            for g in &list {
                log::info!(
                    "[qbz-qt] gpu: device {} '{}' ({})",
                    g.index,
                    g.name,
                    if g.discrete { "discrete" } else { "integrated" }
                );
            }
        }
        list
    })
}

fn enumerate_vulkan_gpus() -> Vec<GpuInfo> {
    use ash::vk;
    // SAFETY: `Entry::load` dlopens libvulkan; every call below is a plain
    // enumeration against the instance we create and destroy here. Nothing
    // escapes this function but owned `String`s and plain numbers, and the
    // instance is destroyed on every path.
    unsafe {
        let Ok(entry) = ash::Entry::load() else {
            return Vec::new();
        };
        let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_0);
        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let Ok(instance) = entry.create_instance(&create_info, None) else {
            return Vec::new();
        };
        let devices = instance.enumerate_physical_devices().unwrap_or_default();
        let mut out = Vec::with_capacity(devices.len());
        for (i, pd) in devices.iter().enumerate() {
            let props = instance.get_physical_device_properties(*pd);
            let name = props
                .device_name_as_c_str()
                .ok()
                .and_then(|s| s.to_str().ok())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            out.push(GpuInfo {
                index: i as u32,
                name,
                discrete: props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU,
            });
        }
        instance.destroy_instance(None);
        out
    }
}

/// The GPU whose index Vulkan would pick with no override — Qt's own default
/// is device 0. Selecting it needs no coupling at all.
fn default_gpu_index() -> u32 {
    0
}

/// Resolve a persisted `gpu_power` value to a device. Accepts a NAME (what
/// this row and the reference both store) and the legacy CLASS keys the older
/// builds and the Slint app write, so a shared pref never reads as garbage.
///
/// `None` = Auto, or a value that matches no present device — which is exactly
/// the case that used to let you select a GPU that does not exist.
pub fn resolve_gpu(pref: &str) -> Option<&'static GpuInfo> {
    let pref = pref.trim();
    if pref.is_empty() || pref == "auto" {
        return None;
    }
    let list = gpus();
    if let Some(hit) = list.iter().find(|g| g.name == pref) {
        return Some(hit);
    }
    // Legacy class keys — resolve to a device that ACTUALLY EXISTS, or to
    // nothing. "discrete" on a single-GPU laptop resolves to None, and that is
    // the point.
    match pref {
        "discrete" => list.iter().find(|g| g.discrete),
        "integrated" => list.iter().find(|g| !g.discrete),
        _ => None,
    }
}

/// Can this machine actually honour a Preferred-GPU choice?
///
/// Two conditions, and BOTH are the owner's point that a control must never
/// offer what does not exist:
/// * the platform can select at all — false on macOS, where QRhi hardcodes
///   `MTLCreateSystemDefaultDevice` (no env, no API);
/// * there is more than one GPU to choose BETWEEN. On a single-GPU box the row
///   is a dropdown with one real answer, so it hides.
pub fn gpu_selectable() -> bool {
    cfg!(target_os = "linux") && gpus().len() > 1
}

/// Apply the persisted Preferred GPU. Called from `apply_renderer_preference`'s
/// slot — BEFORE `QGuiApplication`, because an env set after the graphics
/// context exists is a silent no-op (scoping trap 2).
///
/// Deliberately class-level, exactly like the reference: it nudges the driver
/// toward the integrated or the discrete GPU rather than binding one adapter.
/// Slint has the same limitation (documented F7) and so does Chromium
/// (`--force_high_performance_gpu`) — this is the ceiling of the platform, not
/// a shortcut.
pub fn apply_gpu_preference() {
    if !gpu_selectable() {
        return;
    }
    // An explicit env always wins — someone who exported one of these meant it,
    // and it is the escape hatch when this row guesses wrong.
    for taken in [
        "QT_VK_PHYSICAL_DEVICE_INDEX",
        "DRI_PRIME",
        "__NV_PRIME_RENDER_OFFLOAD",
    ] {
        if std::env::var_os(taken).is_some() {
            log::info!("[qbz-qt] gpu: {taken} already set; leaving the choice to it");
            return;
        }
    }
    // `apply_renderer_preference()` runs FIRST and may have forced a backend.
    // A GPU pick needs Vulkan (see below), so it cannot also honour an explicit
    // `software`/`opengl` choice — the more specific, explicitly-chosen
    // RENDERER wins and this says so rather than quietly overriding it.
    if let Some(forced) = std::env::var_os("QSG_RHI_BACKEND")
        .or_else(|| std::env::var_os("QT_QUICK_BACKEND"))
    {
        log::info!(
            "[qbz-qt] gpu: the renderer row already forced {forced:?}; a GPU pick needs \
             Vulkan, so the explicit renderer choice wins"
        );
        return;
    }
    let pref = crate::settings_qt::pref_str("gpu_power", "auto");
    let Some(gpu) = resolve_gpu(&pref) else {
        if pref != "auto" && !pref.is_empty() {
            log::warn!(
                "[qbz-qt] gpu: '{pref}' matches no device present on this machine -> Auto"
            );
        }
        return;
    };
    if gpu.index == default_gpu_index() {
        // Already the default: selecting it needs no override, and forcing one
        // would buy the Vulkan coupling below for nothing.
        log::info!("[qbz-qt] gpu: '{}' is device 0 (the default) -> no override", gpu.name);
        return;
    }
    // THE VULKAN COUPLING, and why it is the only honest route.
    //
    // MEASURED 2026-08-11 on the Intel-Arc + RTX-4070 hybrid (Wayland,
    // proprietary NVIDIA), with a 6-line QML scene carrying a MultiEffect:
    //
    // | env                | result                                        |
    // |--------------------|-----------------------------------------------|
    // | (none)             | Intel Arc, OpenGL 4.6 desktop — healthy        |
    // | `DRI_PRIME=1`      | **Mesa llvmpipe** — CPU software rendering     |
    // | NVIDIA PRIME pair  | RTX 4070, **OpenGL ES 3.2**                    |
    // | Vulkan + index     | the chosen device, ZERO shader errors          |
    //
    // The GLES context is fatal: Qt's baked `.qsb` shaders carry SPIR-V, GLSL
    // 440 (desktop) and MSL — no GLES variants — so every effect dies with
    // "No GLSL shader code found (versions tried: 320, 310, 300, 100)". That
    // is what killed the NPB Large visualiser when this row first shipped the
    // PRIME envs. `QT_OPENGL=desktop` does not rescue it (measured).
    //
    // Vulkan has none of that problem: the same .qsb already carries SPIR-V,
    // and `QT_VK_PHYSICAL_DEVICE_INDEX` counts the SAME order this module
    // enumerated. So picking a non-default GPU implies the Vulkan backend —
    // stated in the row's own description, never silent.
    std::env::set_var("QSG_RHI_BACKEND", "vulkan");
    std::env::set_var("QT_VK_PHYSICAL_DEVICE_INDEX", gpu.index.to_string());
    log::info!(
        "[qbz-qt] gpu: '{}' -> QSG_RHI_BACKEND=vulkan QT_VK_PHYSICAL_DEVICE_INDEX={}",
        gpu.name,
        gpu.index
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only these two api names may drop the tier. Getting this list wrong is
    /// silent: the app just quietly stops offering the shader scenes.
    #[test]
    fn only_software_and_null_lose_the_gpu_tier() {
        for api in ["opengl", "metal", "vulkan", "d3d11", "d3d12"] {
            assert!(api_is_gpu(api), "{api} should carry the GPU tier");
        }
        for api in ["software", "null"] {
            assert!(!api_is_gpu(api), "{api} must NOT carry the GPU tier");
        }
    }

    /// "unknown" is the value GraphicsInfo reports BEFORE the scene graph
    /// resolves. Latching it as "no GPU" would hide the shader scenes on every
    /// launch until something happened to re-report.
    #[test]
    fn unknown_is_not_a_verdict() {
        assert!(!api_is_gpu("unknown"));
        assert!(!api_is_gpu(""));
        // …and it must not be latchable.
        assert!(!set_active_api("unknown"));
        assert!(!set_active_api("   "));
    }

    /// `GPU_TIER` is a process global and cargo runs tests in PARALLEL, so any
    /// test that MUTATES it takes this first. (Three tests in
    /// `dac_wizard_qt` raced exactly this way on 2026-08-11.)
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Both halves compose; neither can erase the other.
    #[test]
    fn reduce_motion_composes_both_halves() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        GPU_TIER.store(true, Ordering::SeqCst);
        assert!(!reduce_motion(false), "GPU + desktop = full motion");
        assert!(reduce_motion(true), "kiosk forces it on regardless of tier");
        GPU_TIER.store(false, Ordering::SeqCst);
        assert!(reduce_motion(false), "a software tier forces it on by itself");
        assert!(reduce_motion(true));
        GPU_TIER.store(true, Ordering::SeqCst);
    }
}
