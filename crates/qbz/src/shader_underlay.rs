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
//!     with. It builds the pipeline + uniform buffer + bind group (persistent).
//!   * `render_frame()` is called from the 30 fps drain in visualizer.rs while a
//!     shader scene is active. It renders one frame into a FRESH texture (the
//!     documented upstream pattern — avoids read-while-write aliasing) and
//!     returns an `Image`. The caller sets it on `ImmersiveState.shader-texture`.
//!   * `teardown()` clears the state at `RenderingState::RenderingTeardown`.
//!
//! All three run on the UI thread (notifier + Timer share it), so the state lives
//! in a `thread_local`. This file is downstream of the read-only visualizer feed
//! and touches NONE of the protected audio backend.

use std::cell::RefCell;

use slint::wgpu_28::wgpu;
use slint::Image;

/// Offscreen render target size. The `Image` is shown with `image-fit: fill`, so
/// the immersive viewport stretches this to fit. A plasma is organic/soft, so a
/// fixed 720p target is plenty sharp for the spike (HiDPI-correct sizing is a
/// Phase-1 concern, not a spike gate).
const TEX_W: u32 = 1280;
const TEX_H: u32 = 720;
const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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

struct RenderState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// One render pipeline per shader scene, indexed by `mode - 1`:
    ///   pipelines[0] = plasma  (mode 1) — feedback fluid, samples `history`
    ///   pipelines[1] = tunnel  (mode 2)
    ///   pipelines[2] = aurora  (mode 3)
    /// All share one pipeline layout + bind group (uniform + history texture +
    /// sampler); tunnel/aurora ignore bindings 1/2 (a shader using a SUBSET of
    /// the layout is valid). `render_frame` picks the pipeline by index and, for
    /// plasma, copies the frame into `history`.
    pipelines: Vec<wgpu::RenderPipeline>,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Persistent feedback accumulator for the plasma fluid (Direction A). The
    /// plasma shader samples it (binding 1); each plasma frame is copied into it
    /// after the pass, so the next frame advects the previous one.
    history: wgpu::Texture,
    start: std::time::Instant,
}

/// The WGSL source for each scene, in mode order (index = mode - 1). Adding a
/// scene = one `include_str!` here + one entry in the picker (state/UI). All
/// must declare the SAME `Uniforms` block (group0/binding0) as plasma.wgsl.
const SHADER_SOURCES: &[&str] = &[
    include_str!("../../qbz-ui/ui/shaders/plasma.wgsl"),
    include_str!("../../qbz-ui/ui/shaders/tunnel.wgsl"),
    include_str!("../../qbz-ui/ui/shaders/aurora.wgsl"),
];

thread_local! {
    static STATE: RefCell<Option<RenderState>> = const { RefCell::new(None) };
    static PALETTE: RefCell<Palette> = const { RefCell::new(Palette::DEFAULT) };
}

/// Build the persistent pipeline from Slint's device/queue. Called once at
/// `RenderingSetup`. Idempotent-ish: a second call rebuilds (cheap; only happens
/// if the rendering surface is torn down and re-created).
pub fn setup(device: wgpu::Device, queue: wgpu::Queue) {
    // One bind group layout shared by all three pipelines: the uniform buffer
    // (binding 0) plus the feedback history texture (binding 1) + its sampler
    // (binding 2). Only the plasma fluid samples 1/2; tunnel/aurora declare just
    // binding 0 (a pipeline whose shader uses a SUBSET of the layout is valid).
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("qbz-shader-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
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
        ],
    });

    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("qbz-shader-uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Bilinear sampler + persistent history texture for the plasma feedback loop.
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
    let history = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("qbz-shader-history"),
        size: wgpu::Extent3d {
            width: TEX_W,
            height: TEX_H,
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

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("qbz-shader-bg"),
        layout: &bgl,
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
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("qbz-shader-pl"),
        bind_group_layouts: &[&bgl],
        // wgpu 28.x: replaces `push_constant_ranges`. We use none.
        immediate_size: 0,
    });

    // Build one pipeline per scene (plasma + tunnel + aurora). They share the
    // pipeline layout / bind group / uniform buffer / vertex stage; only the
    // fragment shader source differs. `vs_main` / `fs_main` entry points are
    // identical across all three WGSL files (the fullscreen-triangle template).
    let mut pipelines: Vec<wgpu::RenderPipeline> = Vec::with_capacity(SHADER_SOURCES.len());
    for (i, src) in SHADER_SOURCES.iter().enumerate() {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("qbz-shader-module"),
            source: wgpu::ShaderSource::Wgsl((*src).into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("qbz-shader-pipeline"),
            layout: Some(&pipeline_layout),
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
        pipelines.push(pipeline);
        log::debug!("[shader] pipeline {} built (mode {})", i, i + 1);
    }

    let n = pipelines.len();
    STATE.with(|s| {
        *s.borrow_mut() = Some(RenderState {
            device,
            queue,
            pipelines,
            uniform_buf,
            bind_group,
            history,
            start: std::time::Instant::now(),
        });
    });
    log::info!("[shader] wgpu underlay ready ({n} scenes, {TEX_W}x{TEX_H} {TEX_FORMAT:?})");
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

/// Render one frame of scene `mode` into a fresh texture and return it as a
/// Slint `Image`. `mode` is the `ImmersiveState.shader-mode` value (1 = plasma,
/// 2 = tunnel, 3 = aurora); the pipeline index is `mode - 1`. Returns `None`
/// before `setup()` has run, for `mode <= 0`, or for an out-of-range mode
/// (defensive — the UI never sends one). Driven at 30 fps from visualizer.rs.
pub fn render_frame(mode: i32, a: &FrameAudio) -> Option<Image> {
    if mode <= 0 {
        return None;
    }
    STATE.with(|s| {
        let borrow = s.borrow();
        let st = borrow.as_ref()?;

        // Bounds-guard: fall back to the plasma pipeline (index 0) if a mode is
        // ever out of range, so the underlay degrades gracefully instead of
        // panicking on an indexing error.
        let idx = (mode - 1) as usize;
        let pipeline = st.pipelines.get(idx).or_else(|| st.pipelines.first())?;

        let pal = PALETTE.with(|p| *p.borrow());
        let uniforms = Uniforms {
            time: st.start.elapsed().as_secs_f32(),
            phase: a.phase,
            beat: a.beat,
            level: a.level,
            res_x: TEX_W as f32,
            res_y: TEX_H as f32,
            level_smooth: a.level_smooth,
            transient: a.transient,
            energy_lo: [a.energy[0], a.energy[1], a.energy[2], a.energy[3]],
            energy_hi: [a.energy[4], 0.0, 0.0, 0.0],
            bands_lo: [a.bands[0], a.bands[1], a.bands[2], a.bands[3]],
            bands_hi: [a.bands[4], a.bands[5], a.bands[6], a.bands[7]],
            primary: pal.primary,
            secondary: pal.secondary,
            accent: pal.accent,
        };
        st.queue
            .write_buffer(&st.uniform_buf, 0, uniforms_bytes(&uniforms));

        // Fresh target each frame. Image::try_from REQUIRES Rgba8Unorm/Srgb +
        // TEXTURE_BINDING | RENDER_ATTACHMENT (Slint graphics/wgpu_28.rs).
        let texture = st.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("qbz-shader-target"),
            size: wgpu::Extent3d {
                width: TEX_W,
                height: TEX_H,
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
        });
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
            pass.set_bind_group(0, &st.bind_group, &[]);
            pass.draw(0..3, 0..1);
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
                    texture: &st.history,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: TEX_W,
                    height: TEX_H,
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
