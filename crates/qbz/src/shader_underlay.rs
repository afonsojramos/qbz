//! WGPU UNDERLAY SPIKE — GPU fragment-shader background for the ImmersiveView.
//!
//! Validates the renderer-swap path (`renderer-femtovg` GL -> `renderer-femtovg-wgpu`)
//! by rendering a WGSL fragment shader into a wgpu texture and handing it back to
//! Slint as an `Image` (the texture-into-scene shape from Slint's upstream
//! `wgpu_texture` example). The texture is bound to an `Image` placed at the
//! bottom of `ImmersiveView`'s z-stack (see ui/immersive/ImmersiveView.slint).
//!
//! Lifecycle:
//!   * `setup()` is called once by the rendering notifier in main.rs at
//!     `RenderingState::RenderingSetup`, with Slint's OWN wgpu Device/Queue —
//!     mandatory so `Image::try_from` operates on the same device Slint renders
//!     with. It only STASHES them (cheap); pipelines/textures build lazily on
//!     the first frame a shader scene is active (one-time hitch on first open),
//!     and each scene's pipeline compiles on its first use.
//!   * `render_frame()` is called from the 30 fps drain in visualizer.rs while a
//!     shader scene is active AND the immersive view is open. It renders one
//!     frame into the next texture of a rotating 3-deep pool sized to the
//!     window (capped at `TEX_W`x`TEX_H`) and returns an `Image`. The caller
//!     sets it on `ImmersiveState.shader-texture`.
//!   * `teardown()` clears the state at `RenderingState::RenderingTeardown`.
//!
//! All three run on the UI thread (notifier + Timer share it), so the state lives
//! in a `thread_local`. This file is downstream of the read-only visualizer feed
//! and touches NONE of the protected audio backend.

use std::cell::RefCell;

use slint::wgpu_28::wgpu;
use slint::Image;

/// Offscreen render target CEILING. The actual target tracks the window's
/// physical pixel size (no point burning fill rate above it on small screens —
/// Raspberry Pi class hardware) but never exceeds this cap; the `Image` is
/// shown with `image-fit: fill`, so the immersive viewport stretches whatever
/// size to fit.
const TEX_W: u32 = 2560;
const TEX_H: u32 = 1440;
const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Spectral-ribbon spectrogram dims: 512 frequency bands wide × time columns
/// tall (R8). One row is written per new spectral frame at the playback-time
/// column; un-written columns stay zero (the ribbon paints as the track plays).
const SPECTRO_BANDS: u32 = 512;
const SPECTRO_COLS: u32 = 2048;

/// Line-bed lattice: 200 depth lines × 256 frequency points (matches the Tauri
/// LinebedPanel NUM_LINES / VISUAL_BANDS).
const LINEBED_LINES: u32 = 200;
const LINEBED_BANDS: u32 = 256;
/// Each band span is subdivided into LINEBED_SUBDIV Catmull-Rom steps in the
/// vertex shader so the polylines read as smooth curves. MUST match SUBDIV in
/// line_bed.wgsl. Vertex count per line = (LINEBED_BANDS - 1) * SUBDIV + 1.
const LINEBED_SUBDIV: u32 = 6;

/// Mirrors the WGSL `Uniforms` struct in all three `ui/shaders/*.wgsl`. Plain
/// `f32` / `[f32;4]` (align 4) with manual field ordering so the byte offsets
/// match the WGSL std140 layout exactly (every `vec4` lands on a 16-byte
/// boundary; the `res_x`/`res_y` pair is read as a `vec2`), with no vec types or
/// bytemuck needed. 144 bytes = 9×vec4. Offset table:
/// qbz-nix-docs/immersive-shaders-2026-06-28/00-analysis-and-design-spec.md §2.2.
#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    time: f32,           //   0
    phase: f32,          //   4  audio-reactive forward-motion clock (host accumulator)
    beat: f32,           //   8  onset envelope (~0.88 decay) — the "punch"
    level: f32,          //  12  instantaneous overall level = mean(energy bands)
    res_x: f32,          //  16  } WGSL reads these two as `resolution: vec2<f32>`
    res_y: f32,          //  20  }
    level_smooth: f32,   //  24  slow EMA of level (breathing / inertia)
    transient: f32,      //  28  fast transient (*0.85) — kept for the legacy bodies
    energy_lo: [f32; 4], //  32  sub, bass, mid, presence
    energy_hi: [f32; 4], //  48  air, 0, 0, 0
    bands_lo: [f32; 4],  //  64  log bars 0..3
    bands_hi: [f32; 4],  //  80  log bars 4..7
    primary: [f32; 4],   //  96  album-art palette (rgb, a = 1)
    secondary: [f32; 4], // 112
    accent: [f32; 4],    // 128
} // size_of == 144, align 4

// Drift guard: WGSL is compiled at runtime (naga), so cargo cannot catch a
// Rust/WGSL layout mismatch. This catches the Rust side; the WGSL side is the
// manual offset table in the spec (and the Slice-0 canary — the unchanged
// shaders must look identical).
const _: () = assert!(core::mem::size_of::<Uniforms>() == 144);

/// Per-frame audio drivers handed to [`render_frame`] from the 30 fps drain.
/// `time` and resolution come from the render state; the album-art palette is
/// pushed separately via [`set_palette`] (it changes on track change, not per
/// tick). Energy bands and log bands are ALREADY smoothed upstream (qbz-audio)
/// — pass them raw, do not EMA again.
pub struct FrameAudio {
    pub level: f32,
    pub level_smooth: f32,
    pub beat: f32,
    pub phase: f32,
    pub transient: f32,
    pub energy: [f32; 5], // sub, bass, mid, presence, air
    pub bands: [f32; 8],  // 8 log FFT bands (paired from the 16 bars)
    /// Spectral-ribbon feed (mode 4): the latest 512-band frame to paint as a
    /// new column (None = no new frame this tick), the playback fraction 0..1
    /// for the column position, and a reset flag (track change / seek → clear).
    pub spectral: Option<Vec<f32>>,
    pub progress: f32,
    pub reset: bool,
    /// Smoothed fraction (0..1) of the highest active frequency band — drives the
    /// spectral-ribbon real-time ceiling line (mode 4). 0 for the other modes.
    pub spectral_peak: f32,
}

/// Album-art palette triad, normalized rgb (0..1, a = 1). Lives in its own
/// thread-local so a track's colors can be pushed before the render pipeline
/// exists (`set_palette` may run before `setup()`), and read on every frame.
#[derive(Clone, Copy)]
struct Palette {
    primary: [f32; 4],
    secondary: [f32; 4],
    accent: [f32; 4],
}
impl Palette {
    /// Matches the `ImmersiveState` defaults #00dcc8 / #9632ff / #3fd9c8 so a
    /// shader opened before album art resolves still gets sensible colors.
    const DEFAULT: Palette = Palette {
        primary: [0.0, 0.862_745, 0.784_314, 1.0],
        secondary: [0.588_235, 0.196_078, 1.0, 1.0],
        accent: [0.247_059, 0.850_980, 0.784_314, 1.0],
    };
}

/// View `Uniforms` as raw bytes for `Queue::write_buffer`. Sound: `Uniforms` is
/// `#[repr(C)]`, all-`f32`, no padding holes with undefined values we read back —
/// every byte is part of a defined `f32` field.
fn uniforms_bytes(u: &Uniforms) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            (u as *const Uniforms) as *const u8,
            std::mem::size_of::<Uniforms>(),
        )
    }
}

/// View a `&[f32]` as bytes for the heights upload. Same soundness as
/// `uniforms_bytes` — plain `f32`, no padding holes.
fn f32_bytes(s: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

/// Line-bed (mode 5) reshaping + depth ring. Its own thread-local so
/// `render_frame` can mutate it independently of the immutable `STATE` borrow.
struct LineBedState {
    smoothed: Vec<f32>, // 512-band receive-IIR accumulator
    ring: Vec<f32>,     // LINEBED_LINES*LINEBED_BANDS, depth-ordered (row 0 = newest)
}
impl LineBedState {
    fn new() -> Self {
        Self {
            smoothed: vec![0.0; SPECTRO_BANDS as usize],
            ring: vec![0.0; (LINEBED_LINES * LINEBED_BANDS) as usize],
        }
    }
    /// Receive-IIR a 512-band frame, reshape to 256 heights, push at the near row.
    fn push(&mut self, bins: &[f32]) {
        let n = self.smoothed.len().min(bins.len());
        for i in 0..n {
            self.smoothed[i] = self.smoothed[i] * 0.03 + bins[i] * 0.97;
        }
        let row = reshape_512_to_256(&self.smoothed);
        let bands = LINEBED_BANDS as usize;
        let lines = LINEBED_LINES as usize;
        // Shift every row one slot deeper, then write the newest at row 0.
        self.ring.copy_within(0..(lines - 1) * bands, bands);
        self.ring[0..bands].copy_from_slice(&row);
    }
}
thread_local! {
    static LINEBED: RefCell<LineBedState> = RefCell::new(LineBedState::new());
}
thread_local! {
    /// Last spectrogram column written (spectral-ribbon gap-fill).
    static SPECTRO_LAST_COL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Reused upload scratch for the spectral-ribbon column writes (mode 4):
    /// (one quantized 512-band row, the gap-fill repetition of it). Avoids two
    /// Vec allocations per spectral frame.
    static SPECTRO_SCRATCH: RefCell<(Vec<u8>, Vec<u8>)> =
        const { RefCell::new((Vec::new(), Vec::new())) };
}

/// 512 backend bands → 256 line heights in [0.1, 84] — Tauri's LinebedPanel
/// chain (the backend bands are intentionally flat; this is what makes the
/// ridges): frequency-warp bin map → peak-preserving smoothing → low-end tail
/// roll-off → 3-point box → per-band gamma + soft clip.
fn reshape_512_to_256(data: &[f32]) -> [f32; 256] {
    let mut vis = [0.0f32; 256];
    for i in 0..256 {
        let seg_start = (i as f32 / 256.0).powf(1.32);
        let seg_end = ((i + 1) as f32 / 256.0).powf(1.32);
        let s = 4.0 + (460.0 - 4.0) * seg_start;
        let e = 4.0 + (460.0 - 4.0) * seg_end;
        let lower = (s.floor() as usize).max(4);
        let upper = (e.ceil() as usize).min(460);
        let (mut sum, mut peak, mut cnt) = (0.0f32, 0.0f32, 0u32);
        let mut j = lower;
        while j <= upper && j < data.len() {
            sum += data[j];
            if data[j] > peak {
                peak = data[j];
            }
            cnt += 1;
            j += 1;
        }
        let avg = if cnt > 0 { sum / cnt as f32 } else { 0.0 };
        vis[i] = (avg * 0.52 + peak * 0.48) * 770.0;
    }
    apply_average(&mut vis);
    // Low-end tail roll-off (first 7 bins).
    for i in 0..7usize {
        vis[i] *= 0.013_334_120_966_221_101 * ((i + 1) as f32).powf(1.6) + 0.7;
    }
    smooth3(&mut vis);
    // Per-band gamma + soft clip + cap → [0.1, 84].
    for i in 0..256 {
        let frac = i as f32 / 255.0;
        let exp = 1.35 + (0.9 - 1.35) * frac * frac;
        let norm = (vis[i] / 770.0).max(0.0);
        let shaped = norm.powf(exp);
        let comp = 1.0 - (-shaped * 3.25).exp();
        vis[i] = (comp * 84.0).clamp(0.1, 84.0);
    }
    vis
}

/// Two-pass peak-preserving smoothing (Tauri applyAverageTransform).
fn apply_average(d: &mut [f32; 256]) {
    let src = *d;
    for i in 0..256 {
        let prev = if i > 0 { src[i - 1] } else { src[i] };
        let next = if i < 255 { src[i + 1] } else { src[i] };
        let cur = src[i];
        d[i] = if cur >= prev && cur >= next {
            cur
        } else {
            (cur + prev.max(next)) / 2.0
        };
    }
    let src2 = *d;
    for i in 0..256 {
        let prev = if i > 0 { src2[i - 1] } else { src2[i] };
        let next = if i < 255 { src2[i + 1] } else { src2[i] };
        let cur = src2[i];
        d[i] = if cur >= prev && cur >= next {
            cur
        } else {
            cur / 2.0 + prev.max(next) / 3.0 + prev.min(next) / 6.0
        };
    }
}

/// 3-point box smooth, one pass (Tauri smoothSpectrum).
fn smooth3(d: &mut [f32; 256]) {
    let src = *d;
    for i in 0..256 {
        let prev = if i > 0 { src[i - 1] } else { src[i] };
        let next = if i < 255 { src[i + 1] } else { src[i] };
        d[i] = (prev + src[i] + next) / 3.0;
    }
}

/// GPU resources whose size tracks the window (recreated on resize by
/// `render_frame`): the plasma history accumulator, the bind group (the VIEW of
/// `history` is baked into it), and the rotating pool of offscreen targets.
struct SizedResources {
    /// Clamped target size: min(window physical size, TEX_W x TEX_H).
    size: (u32, u32),
    /// Persistent feedback accumulator for the plasma fluid (Direction A). The
    /// plasma shader samples it (binding 1); each plasma frame is copied into it
    /// after the pass, so the next frame advects the previous one.
    history: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    /// Rotating 3-deep pool of offscreen targets (replaces a fresh 14.7 MB
    /// texture per frame). Safe: Slint's femtovg renderer holds only the
    /// CURRENT frame's texture (per-item graphics cache keyed on the `source`
    /// property; `ImageCacheKey` is None for WGPU textures so nothing lingers
    /// in the shared texture cache — see vendored femtovg images.rs) — so a
    /// pool texture is re-rendered ~2 ticks after Slint stopped referencing
    /// it, and same-queue submission ordering serializes any residual GPU
    /// reads regardless.
    targets: [wgpu::Texture; 3],
    next_target: usize,
}

/// Everything built lazily on the FIRST frame a shader scene is active —
/// compiling six WGSL pipelines + allocating the history/spectrogram textures
/// at first window paint costs startup time and VRAM even when the immersive
/// shaders are never opened. Scene pipelines are additionally per-scene lazy.
struct GpuResources {
    bgl: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    uniform_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// Persistent spectrogram for the spectral-ribbon scene (binding 3); written
    /// one column per spectral frame, sampled for display. Fixed size.
    spectrogram: wgpu::Texture,
    /// Line-bed (mode 5) 256×200 heights texture (binding 4). Fixed size.
    heights_tex: wgpu::Texture,
    /// One render pipeline per fullscreen shader scene, indexed like
    /// `SHADER_SOURCES` (modes 1-4 → 0..3, mode 6 → 4), each compiled+cached on
    /// its first use. All share one pipeline layout + bind group (uniform +
    /// history texture + sampler); scenes ignore the bindings they don't
    /// declare (a shader using a SUBSET of the layout is valid). `render_frame`
    /// picks the pipeline by index and, for plasma, copies the frame into
    /// `history`.
    pipelines: Vec<Option<wgpu::RenderPipeline>>,
    /// Line-bed (mode 5): its own line-strip pipeline, also lazy.
    linebed_pipeline: Option<wgpu::RenderPipeline>,
    sized: SizedResources,
}

struct RenderState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    start: std::time::Instant,
    /// None until the first `render_frame` with a shader scene active.
    res: Option<GpuResources>,
}

/// The WGSL source for each scene, in mode order (index = mode - 1). Adding a
/// scene = one `include_str!` here + one entry in the picker (state/UI). All
/// must declare the SAME `Uniforms` block (group0/binding0) as plasma.wgsl.
const SHADER_SOURCES: &[&str] = &[
    include_str!("../../qbz-ui/ui/shaders/plasma.wgsl"),          // [0] mode 1
    include_str!("../../qbz-ui/ui/shaders/tunnel.wgsl"),          // [1] mode 2
    include_str!("../../qbz-ui/ui/shaders/aurora.wgsl"),          // [2] mode 3
    include_str!("../../qbz-ui/ui/shaders/spectral_ribbon.wgsl"), // [3] mode 4
    include_str!("../../qbz-ui/ui/shaders/liquid_spectrum.wgsl"), // [4] mode 6
    include_str!("../../qbz-ui/ui/shaders/ambient.wgsl"),         // [5] mode 7 (app-wide)
];

thread_local! {
    static STATE: RefCell<Option<RenderState>> = const { RefCell::new(None) };
    static PALETTE: RefCell<Palette> = const { RefCell::new(Palette::DEFAULT) };
}

/// Stash Slint's device/queue. Called once at `RenderingSetup`. Deliberately
/// CHEAP: no WGSL compilation, no texture allocation — those happen lazily in
/// `render_frame` on first shader use, so sessions that never open a shader
/// scene pay nothing at window paint. A second call re-stashes and drops any
/// built resources (only happens if the rendering surface is re-created).
pub fn setup(device: wgpu::Device, queue: wgpu::Queue) {
    STATE.with(|s| {
        *s.borrow_mut() = Some(RenderState {
            device,
            queue,
            start: std::time::Instant::now(),
            res: None,
        });
    });
    log::info!("[shader] wgpu device/queue captured (GPU resources build on first shader use)");
}

/// Build the shared (size-independent) GPU resources plus the initial sized
/// set. Runs once, on the first frame a shader scene is active.
fn build_shared(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) -> GpuResources {
    // One bind group layout shared by all pipelines: the uniform buffer
    // (binding 0) plus the feedback history texture (binding 1) + its sampler
    // (binding 2). Only the plasma fluid samples 1/2; tunnel/aurora declare just
    // binding 0 (a pipeline whose shader uses a SUBSET of the layout is valid).
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("qbz-shader-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                // VERTEX too: the line-bed vertex shader reads `resolution`.
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // Binding 3: the spectral-ribbon spectrogram (R8). Only that scene
            // samples it; the others declare a subset of the layout (valid).
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // Binding 4: the line-bed heights as an R32Float TEXTURE (256 band ×
            // 200 line), read via textureLoad in the VERTEX stage by line_bed.wgsl.
            // A SAMPLED texture (not a storage buffer) so it works without the
            // VERTEX_STORAGE downlevel capability that Slint's device lacks (a
            // vertex storage buffer fails BGL creation: limit is 0). Others ignore.
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });

    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("qbz-shader-uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Bilinear sampler for the plasma feedback history (the history texture
    // itself is size-dependent and lives in build_sized).
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("qbz-shader-feedback-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    // Spectral-ribbon spectrogram: 512 freq bands (width) × SPECTRO_COLS time
    // columns (height), R8. Written one row per spectral frame in render_frame,
    // sampled by spectral_ribbon.wgsl at binding 3.
    let spectrogram = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("qbz-shader-spectrogram"),
        size: wgpu::Extent3d {
            width: SPECTRO_BANDS,
            height: SPECTRO_COLS,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    // Line-bed heights: an R32Float texture (256 band wide × 200 line tall,
    // depth-ordered rows), uploaded per frame in the mode-5 path and read via
    // textureLoad in the line_bed vertex shader. A sampled texture avoids the
    // vertex-stage storage-buffer limit (0) on Slint's downlevel device.
    let heights_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("qbz-linebed-heights"),
        size: wgpu::Extent3d {
            width: LINEBED_BANDS,
            height: LINEBED_LINES,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("qbz-shader-pl"),
        bind_group_layouts: &[&bgl],
        // wgpu 28.x: replaces `push_constant_ranges`. We use none.
        immediate_size: 0,
    });

    // Scene pipelines compile on their first use (see render_frame); start empty.
    let mut pipelines: Vec<Option<wgpu::RenderPipeline>> =
        Vec::with_capacity(SHADER_SOURCES.len());
    pipelines.resize_with(SHADER_SOURCES.len(), || None);

    let sized = build_sized(
        device,
        queue,
        &bgl,
        &uniform_buf,
        &sampler,
        &spectrogram,
        &heights_tex,
        w,
        h,
    );

    GpuResources {
        bgl,
        pipeline_layout,
        uniform_buf,
        sampler,
        spectrogram,
        heights_tex,
        pipelines,
        linebed_pipeline: None,
        sized,
    }
}

/// (Re)build the window-size-tracking resources: the plasma history
/// accumulator, the rotating target pool, and the bind group (the history VIEW
/// is baked into it, so a size change forces a bind-group rebuild). A resize
/// drops the plasma feedback content — it re-accumulates within a few frames.
#[allow(clippy::too_many_arguments)]
fn build_sized(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    uniform_buf: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    spectrogram: &wgpu::Texture,
    heights_tex: &wgpu::Texture,
    w: u32,
    h: u32,
) -> SizedResources {
    let history = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("qbz-shader-history"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEX_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let history_view = history.create_view(&wgpu::TextureViewDescriptor::default());

    // Clear the accumulator once so the first plasma frame samples black, not
    // uninitialized GPU memory.
    {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("qbz-shader-history-clear"),
        });
        {
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("qbz-shader-history-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &history_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        queue.submit(Some(enc.finish()));
    }

    let spectrogram_view = spectrogram.create_view(&wgpu::TextureViewDescriptor::default());
    let heights_view = heights_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("qbz-shader-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&history_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&spectrogram_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&heights_view),
            },
        ],
    });

    // The rotating offscreen pool. Image::try_from REQUIRES Rgba8Unorm/Srgb +
    // TEXTURE_BINDING | RENDER_ATTACHMENT (Slint graphics/wgpu_28.rs); COPY_SRC
    // feeds the plasma history copy.
    let make_target = || {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("qbz-shader-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let targets = [make_target(), make_target(), make_target()];

    SizedResources {
        size: (w, h),
        history,
        bind_group,
        targets,
        next_target: 0,
    }
}

/// Compile one fullscreen scene pipeline (`SHADER_SOURCES[idx]`). All scenes
/// share the pipeline layout / bind group / uniform buffer / vertex stage; only
/// the fragment shader source differs. `vs_main` / `fs_main` entry points are
/// identical across the scene WGSL files (the fullscreen-triangle template).
fn build_scene_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    idx: usize,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("qbz-shader-module"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SOURCES[idx].into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("qbz-shader-pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: TEX_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    log::debug!("[shader] scene pipeline {idx} built");
    pipeline
}

/// Line-bed (mode 5): a SEPARATE pipeline — line-strip topology + alpha blend
/// + the projecting vertex shader. Shares the pipeline layout / bind group.
fn build_linebed_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    let linebed_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("qbz-linebed-module"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../qbz-ui/ui/shaders/line_bed.wgsl").into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("qbz-linebed-pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: &linebed_module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &linebed_module,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: TEX_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Drop the pipeline at surface teardown.
pub fn teardown() {
    STATE.with(|s| *s.borrow_mut() = None);
    log::info!("[shader] wgpu underlay torn down");
}

/// Push the album-art palette triad. Called on track change (playback.rs), not
/// per frame, from the UI thread. Stored in a thread-local independent of the
/// render pipeline so it survives if pushed before `setup()`.
pub fn set_palette(primary: slint::Color, secondary: slint::Color, accent: slint::Color) {
    fn norm(c: slint::Color) -> [f32; 4] {
        [
            c.red() as f32 / 255.0,
            c.green() as f32 / 255.0,
            c.blue() as f32 / 255.0,
            1.0,
        ]
    }
    PALETTE.with(|p| {
        *p.borrow_mut() = Palette {
            primary: norm(primary),
            secondary: norm(secondary),
            accent: norm(accent),
        };
    });
}

/// Render one frame of scene `mode` into the next pool target and return it as
/// a Slint `Image`. `mode` is the `ImmersiveState.shader-mode` value (1 =
/// plasma, 2 = tunnel, 3 = aurora, ...). `win_w`/`win_h` is the window's
/// PHYSICAL pixel size: the offscreen target is clamped to it (capped at
/// `TEX_W`x`TEX_H`) and the pool is rebuilt when it changes. Returns `None`
/// before `setup()` has run, for `mode <= 0`, or for a zero-sized window.
/// Driven at 30 fps from visualizer.rs while the immersive view is open with a
/// shader scene active.
pub fn render_frame(mode: i32, a: &FrameAudio, win_w: u32, win_h: u32) -> Option<Image> {
    if mode <= 0 || win_w == 0 || win_h == 0 {
        return None;
    }
    STATE.with(|s| {
        let mut borrow = s.borrow_mut();
        let st = borrow.as_mut()?;
        let (tw, th) = (win_w.min(TEX_W), win_h.min(TEX_H));

        // Lazy one-time build of the shared GPU resources (first shader open).
        if st.res.is_none() {
            let t0 = std::time::Instant::now();
            st.res = Some(build_shared(&st.device, &st.queue, tw, th));
            log::info!(
                "[shader] GPU resources built lazily in {:?} ({tw}x{th} {TEX_FORMAT:?})",
                t0.elapsed()
            );
        }
        let res = st.res.as_mut()?;
        if res.sized.size != (tw, th) {
            res.sized = build_sized(
                &st.device,
                &st.queue,
                &res.bgl,
                &res.uniform_buf,
                &res.sampler,
                &res.spectrogram,
                &res.heights_tex,
                tw,
                th,
            );
            log::info!("[shader] render targets resized to {tw}x{th}");
        }

        // Rotate the target pool BEFORE taking the pipeline reference (the
        // pipeline borrow below is shared; this is the last `res` mutation
        // besides the lazy pipeline builds).
        let texture = res.sized.targets[res.sized.next_target].clone();
        res.sized.next_target = (res.sized.next_target + 1) % res.sized.targets.len();

        // Pick (and lazily compile+cache) the pipeline: line-bed (mode 5) uses
        // its own line-strip pipeline; the fullscreen scenes map mode →
        // SHADER_SOURCES index. Bounds-guard: fall back to the plasma pipeline
        // (index 0) if a mode is ever out of range, so the underlay degrades
        // gracefully instead of panicking on an indexing error.
        let pipeline: &wgpu::RenderPipeline = if mode == 5 {
            if res.linebed_pipeline.is_none() {
                res.linebed_pipeline =
                    Some(build_linebed_pipeline(&st.device, &res.pipeline_layout));
            }
            res.linebed_pipeline.as_ref()?
        } else {
            // modes 1-4 → pipelines[0..3]; mode 6 (liquid spectrum) → pipelines[4];
            // mode 7 (ambient, app-wide) → pipelines[5]. mode 5 is line_bed's own
            // pipeline above, so the index skips it.
            let idx = if mode == 6 {
                4
            } else if mode == 7 {
                5
            } else {
                (mode - 1) as usize
            };
            let idx = if idx < SHADER_SOURCES.len() { idx } else { 0 };
            if res.pipelines[idx].is_none() {
                res.pipelines[idx] =
                    Some(build_scene_pipeline(&st.device, &res.pipeline_layout, idx));
            }
            res.pipelines[idx].as_ref()?
        };

        let pal = PALETTE.with(|p| *p.borrow());
        let uniforms = Uniforms {
            time: st.start.elapsed().as_secs_f32(),
            phase: a.phase,
            beat: a.beat,
            level: a.level,
            res_x: tw as f32,
            res_y: th as f32,
            level_smooth: a.level_smooth,
            transient: a.transient,
            energy_lo: [a.energy[0], a.energy[1], a.energy[2], a.energy[3]],
            energy_hi: [a.energy[4], a.spectral_peak, 0.0, 0.0],
            bands_lo: [a.bands[0], a.bands[1], a.bands[2], a.bands[3]],
            bands_hi: [a.bands[4], a.bands[5], a.bands[6], a.bands[7]],
            primary: pal.primary,
            secondary: pal.secondary,
            accent: pal.accent,
        };
        st.queue
            .write_buffer(&res.uniform_buf, 0, uniforms_bytes(&uniforms));

        // Spectral ribbon (mode 4): feed the persistent spectrogram before the
        // display pass. Reset (clear) on track-change/seek, then write the new
        // 512-band column at the playback-time position (paint-as-you-play).
        if mode == 4 {
            if a.reset {
                // Full-texture clear — rare (track change / seek only), so the
                // 1 MB zero buffer is not worth keeping around.
                let zeros = vec![0u8; (SPECTRO_BANDS * SPECTRO_COLS) as usize];
                st.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &res.spectrogram,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &zeros,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(SPECTRO_BANDS),
                        rows_per_image: Some(SPECTRO_COLS),
                    },
                    wgpu::Extent3d {
                        width: SPECTRO_BANDS,
                        height: SPECTRO_COLS,
                        depth_or_array_layers: 1,
                    },
                );
                SPECTRO_LAST_COL.with(|c| c.set(0));
            }
            if let Some(ref bins) = a.spectral {
                if !bins.is_empty() {
                    let col = (a.progress.clamp(0.0, 1.0) * (SPECTRO_COLS - 1) as f32) as u32;
                    let n = SPECTRO_BANDS as usize;
                    SPECTRO_SCRATCH.with(|scratch| {
                        let (row, data) = &mut *scratch.borrow_mut();
                        row.clear();
                        row.resize(n, 0);
                        for (i, slot) in row.iter_mut().enumerate() {
                            if i < bins.len() {
                                *slot = (bins[i].clamp(0.0, 1.0) * 255.0) as u8;
                            }
                        }
                        // Gap-fill: paint every column skipped since the last write
                        // (progress updates ~1 Hz, so the column jumps several slots).
                        let last = SPECTRO_LAST_COL.with(|c| c.get());
                        let start = if col > last { last + 1 } else { col };
                        let count = col + 1 - start;
                        data.clear();
                        data.reserve(n * count as usize);
                        for _ in 0..count {
                            data.extend_from_slice(&row[..]);
                        }
                        st.queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &res.spectrogram,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x: 0, y: start, z: 0 },
                                aspect: wgpu::TextureAspect::All,
                            },
                            &data[..],
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(SPECTRO_BANDS),
                                rows_per_image: Some(count),
                            },
                            wgpu::Extent3d {
                                width: SPECTRO_BANDS,
                                height: count,
                                depth_or_array_layers: 1,
                            },
                        );
                        SPECTRO_LAST_COL.with(|c| c.set(col));
                    });
                }
            }
        }

        // Line bed (mode 5): push the new spectral frame into the depth ring,
        // reshape it (Tauri's 512→256 chain), and upload the 200×256 heights.
        if mode == 5 {
            if let Some(ref bins) = a.spectral {
                if !bins.is_empty() {
                    LINEBED.with(|lb| lb.borrow_mut().push(bins));
                }
            }
            LINEBED.with(|lb| {
                let lb = lb.borrow();
                st.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &res.heights_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    f32_bytes(&lb.ring),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(LINEBED_BANDS * 4),
                        rows_per_image: Some(LINEBED_LINES),
                    },
                    wgpu::Extent3d {
                        width: LINEBED_BANDS,
                        height: LINEBED_LINES,
                        depth_or_array_layers: 1,
                    },
                );
            });
        }

        // Render into the pool texture picked above (a clone = the same
        // underlying wgpu texture; Image::try_from consumes our handle while
        // the pool keeps its own).
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = st
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("qbz-shader-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("qbz-shader-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                // wgpu 28.x render passes also carry the multiview layer mask;
                // we don't use multiview (single 2D target), so None.
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &res.sized.bind_group, &[]);
            if mode == 5 {
                // 200 instanced line strips, each a subdivided Catmull-Rom curve
                // of (255 * SUBDIV + 1) points.
                pass.draw(0..((LINEBED_BANDS - 1) * LINEBED_SUBDIV + 1), 0..LINEBED_LINES);
            } else {
                pass.draw(0..3, 0..1);
            }
        }
        // Plasma fluid (mode 1) feeds back: copy this frame into the history
        // accumulator so the next frame advects it. Tunnel/aurora skip this.
        if mode == 1 {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &res.sized.history,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
            );
        }
        st.queue.submit(Some(encoder.finish()));

        match Image::try_from(texture) {
            Ok(img) => Some(img),
            Err(e) => {
                log::warn!("[shader] Image::try_from failed: {e:?}");
                None
            }
        }
    })
}
