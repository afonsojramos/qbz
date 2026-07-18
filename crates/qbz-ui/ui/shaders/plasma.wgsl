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

// INTEGER lattice hash (bit-exact) — vnoise only hashes floor() outputs, so the
// i32 conversion is exact. A float hash leaves the result to f32 rounding, which
// NVIDIA can contract/round differently per inline call site, so the SAME
// lattice corner hashed from two adjacent cells disagrees and draws the cell
// border as a straight "wallpaper join" seam. u32 arithmetic is bit-exact: same
// input, same output, every call site, every GPU, any coordinate magnitude.
fn hash2(p: vec2<f32>) -> f32 {
    var h = bitcast<u32>(i32(p.x)) * 0x8da6b343u + bitcast<u32>(i32(p.y)) * 0xd8163841u;
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    h = h ^ (h >> 15u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
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

    let pres = clamp(u.energy_lo.w, 0.0, 1.0);

    // === Feedback advection — turbulent, music-cranked. ===
    // Bass inhale/zoom (stronger) + a beat kick.
    let zoom = 1.0 + 0.006 + (sub + bass) * 0.03 + beat * 0.02;
    // Mids swirl harder and faster; beats jolt the rotation.
    let ang = (0.006 + mids * 0.05) * sin(t * 0.5 + length(c) * 4.0) + beat * 0.035;
    let ca = cos(ang);
    let sa = sin(ang);
    var w = c * zoom;
    w = vec2<f32>(w.x * ca - w.y * sa, w.x * sa + w.y * ca);
    // Two curl-noise octaves (treble + presence) → filaments + fine turbulence.
    w += curl_noise(c * 2.5 + vec2<f32>(t * 0.12, -t * 0.10)) * (0.003 + air * 0.015);
    w += curl_noise(c * 5.5 - vec2<f32>(t * 0.09, t * 0.08)) * (0.0015 + pres * 0.009);
    let warpUv = w / vec2<f32>(aspect, 1.0) + vec2<f32>(0.5);

    var field = textureSample(prev_tex, prev_samp, warpUv).rgb;

    // Decay: faster so it stays ALIVE (not muddy); louder lingers a bit longer.
    field *= 0.85 + lvl * 0.06;

    // === FOUR emitters, fast orbits — more elements, more motion. ===
    let e1 = vec2<f32>(sin(t * 0.5), cos(t * 0.6)) * (0.25 + bass * 0.2);
    let e2 = vec2<f32>(sin(-t * 0.42 + 2.1), cos(-t * 0.38 + 1.3)) * (0.3 + mids * 0.18);
    let e3 = vec2<f32>(sin(t * 0.74 + 1.0), cos(-t * 0.66 + 3.0)) * (0.22 + pres * 0.2);
    let e4 = vec2<f32>(cos(t * 0.33 + 4.0), sin(t * 0.58 + 0.5)) * (0.34 + air * 0.16);
    field += u.primary.rgb * blob(c, e1, 0.045 + bass * 0.06) * (0.14 + level * 0.8);
    field += u.secondary.rgb * blob(c, e2, 0.05 + mids * 0.05) * (0.14 + level * 0.65);
    field += u.accent.rgb * blob(c, e3, 0.04 + pres * 0.05) * (0.10 + level * 0.6);
    field += u.primary.rgb * blob(c, e4, 0.035 + air * 0.04) * (0.07 + air * 0.6);

    // === Beat splats — TWO, bigger and brighter, detonating on the onset. ===
    let sp1 = vec2<f32>(sin(t * 0.9) * 0.35, cos(t * 0.7) * 0.28);
    let sp2 = vec2<f32>(cos(t * 1.1 + 2.0) * 0.3, sin(t * 0.85 + 1.0) * 0.32);
    field += u.accent.rgb * blob(c, sp1, 0.05 + beat * 0.14) * beat * 2.2;
    field += u.secondary.rgb * blob(c, sp2, 0.04 + beat * 0.10) * beat * 1.6;

    // Treble shimmer on the crests.
    let luma = dot(field, vec3<f32>(0.33, 0.34, 0.33));
    field += u.accent.rgb * air * smoothstep(0.4, 0.9, luma) * 0.18;

    field = clamp(field, vec3<f32>(0.0), vec3<f32>(1.3));
    return vec4<f32>(field, 1.0);
}
