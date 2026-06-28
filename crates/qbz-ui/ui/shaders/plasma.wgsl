// FEEDBACK FLUID PLASMA (MilkDrop-class) — original clean-room fragment shader.
//
// A living liquid light field: each frame samples the PREVIOUS frame at a warped
// UV (advection) — bass inhales toward center, mids rotate the swirl, treble adds
// curl-noise filaments — multiplies it by a decay, then injects new palette-color
// ink and a beat-driven accent bloom that flows away on the current. The "trails
// you can't tell where motion ends" MilkDrop signature.
//
// Feedback path: src/shader_underlay.rs keeps a persistent `history` texture; the
// plasma pipeline samples it (binding 1) via a bilinear sampler (binding 2), and
// each plasma frame is copied back into `history` after the pass. The Uniforms
// block is byte-identical to the `Uniforms` #[repr(C)] struct (std140, 144 bytes).
// ENTIRELY original code (technique-only from MilkDrop, public) — Flathub-clean.

struct Uniforms {
    time: f32,
    phase: f32,
    beat: f32,
    level: f32,
    resolution: vec2<f32>,
    level_smooth: f32,
    transient: f32,
    energy_lo: vec4<f32>,
    energy_hi: vec4<f32>,
    bands_lo: vec4<f32>,
    bands_hi: vec4<f32>,
    primary: vec4<f32>,
    secondary: vec4<f32>,
    accent: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var prev_tex: texture_2d<f32>;
@group(0) @binding(2) var prev_samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var verts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    let p = verts[vid];
    var out: VsOut;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    let uf = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, uf.x), mix(c, d, uf.x), uf.y);
}

// Divergence-free flow from a scalar potential: curl = (dphi/dy, -dphi/dx).
fn curl_noise(p: vec2<f32>) -> vec2<f32> {
    let e = 0.05;
    let dx = vnoise(p + vec2<f32>(e, 0.0)) - vnoise(p - vec2<f32>(e, 0.0));
    let dy = vnoise(p + vec2<f32>(0.0, e)) - vnoise(p - vec2<f32>(0.0, e));
    return vec2<f32>(dy, -dx) / (2.0 * e);
}

// Soft gaussian blob — an ink emitter.
fn blob(p: vec2<f32>, c: vec2<f32>, radius: f32) -> f32 {
    let d = p - c;
    return exp(-dot(d, d) / max(radius * radius, 1e-4));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = u.time;
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);

    let sub = clamp(u.energy_lo.x, 0.0, 1.0);
    let bass = clamp(u.energy_lo.y, 0.0, 1.0);
    let mids = clamp(u.energy_lo.z, 0.0, 1.0);
    let air = clamp(u.energy_hi.x, 0.0, 1.0);
    let beat = clamp(u.beat, 0.0, 1.0);
    let level = clamp(u.level, 0.0, 1.0);
    let lvl = clamp(u.level_smooth, 0.0, 1.0);

    // Centered, aspect-correct coords.
    let c = (in.uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);

    // === Feedback advection: warp where we read the previous frame. ===
    // Bass inhales toward center (sample farther out → content contracts).
    let zoom = 1.0 + 0.004 + (sub + bass) * 0.012;
    // Mids rotate/shear the swirl.
    let ang = (0.003 + mids * 0.02) * sin(t * 0.2 + length(c) * 3.0);
    let ca = cos(ang);
    let sa = sin(ang);
    var w = c * zoom;
    w = vec2<f32>(w.x * ca - w.y * sa, w.x * sa + w.y * ca);
    // Treble = curl-noise filaments.
    w += curl_noise(c * 2.5 + vec2<f32>(t * 0.05, -t * 0.04)) * (0.0015 + air * 0.006);
    let warpUv = w / vec2<f32>(aspect, 1.0) + vec2<f32>(0.5);

    var field = textureSample(prev_tex, prev_samp, warpUv).rgb;

    // Decay: louder = slower decay (ink lingers); quiet = calm/sparse.
    field *= 0.90 + lvl * 0.075;

    // === New ink: two counter-rotating emitters in primary / secondary. ===
    let e1 = vec2<f32>(sin(t * 0.27), cos(t * 0.31)) * (0.22 + bass * 0.18);
    let e2 = vec2<f32>(sin(-t * 0.21 + 2.1), cos(-t * 0.19 + 1.3)) * (0.26 + mids * 0.16);
    field += u.primary.rgb * blob(c, e1, 0.05 + bass * 0.06) * (0.10 + level * 0.55);
    field += u.secondary.rgb * blob(c, e2, 0.06 + mids * 0.05) * (0.10 + level * 0.45);

    // === Beat splat: accent bloom that flows away on the current. ===
    let splatPos = vec2<f32>(sin(t * 0.7) * 0.3, cos(t * 0.53) * 0.24);
    field += u.accent.rgb * blob(c, splatPos, 0.04 + beat * 0.10) * beat * 1.3;

    // Treble shimmer on the crests.
    let luma = dot(field, vec3<f32>(0.33, 0.34, 0.33));
    field += u.accent.rgb * air * smoothstep(0.45, 0.9, luma) * 0.12;

    field = clamp(field, vec3<f32>(0.0), vec3<f32>(1.2));
    return vec4<f32>(field, 1.0);
}
