// AURORA WARP — original clean-room fragment shader (Checkpoint E).
//
// Flowing horizontal aurora bands driven by stacked, domain-warped sine
// fields (a cheap fake-curl flow). Bass (u.energy0) widens and brightens the
// curtains; a transient (u.transient) adds a brief vertical shimmer bloom.
// One fullscreen-triangle pass, fixed work per pixel — GPU-cheap, no loops.
//
// CPU side: src/shader_underlay.rs. The Uniforms block here is byte-identical
// to the `Uniforms` #[repr(C)] struct there (vec4-aligned, 32 bytes). This is
// ENTIRELY original code (no external source copied) per the Flathub license
// rule. Designed to look clearly different from plasma.wgsl and tunnel.wgsl.

struct Uniforms {
    time: f32,
    energy0: f32,
    transient: f32,
    _pad0: f32,
    resolution: vec2<f32>,
    _pad1: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

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

// A small smooth flow field: two cheap sine lobes that act like a fake curl,
// so the curtains ripple horizontally instead of sliding rigidly.
fn flow(p: vec2<f32>, t: f32) -> f32 {
    let a = sin(p.x * 2.3 + t * 0.7) * 0.5;
    let b = sin(p.x * 4.7 - t * 0.45 + p.y * 1.3) * 0.25;
    let c = sin((p.x + p.y) * 1.7 + t * 0.9) * 0.18;
    return a + b + c;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = u.time;
    let bass = clamp(u.energy0, 0.0, 1.0);
    let tr = clamp(u.transient, 0.0, 1.0);

    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    // x spread by aspect so the curtains aren't squashed on wide windows.
    var p = vec2<f32>((in.uv.x - 0.5) * aspect, in.uv.y);

    // Two domain-warped curtains at different vertical anchors. Each band is a
    // narrow glow around a warped horizontal line; the warp comes from `flow`.
    let warp1 = flow(p * 3.0, t);
    let warp2 = flow(p * 2.0 + vec2<f32>(7.3, 0.0), t * 0.8 + 2.0);

    // Band height widens with bass so loud passages bloom the curtains.
    let bw = 0.10 + bass * 0.14;

    let center1 = 0.42 + warp1 * (0.10 + bass * 0.07);
    let center2 = 0.62 + warp2 * (0.12 + bass * 0.08);

    let band1 = bw / (abs(p.y - center1) + bw);
    let band2 = bw / (abs(p.y - center2) + bw);

    // Two aurora hues (greenish-teal and magenta-violet) that breathe with t.
    let teal = vec3<f32>(0.10, 0.95, 0.65);
    let violet = vec3<f32>(0.65, 0.25, 1.00);
    // Slow hue shimmer.
    let shimmer = 0.5 + 0.5 * sin(p.x * 5.0 + t * 1.3);

    var col = teal * band1 * (0.55 + shimmer * 0.45);
    col = col + violet * band2 * (0.55 + (1.0 - shimmer) * 0.45);

    // Bass lifts overall brightness; gentle floor glow so it's never pure black.
    col = col * (0.6 + bass * 1.0);
    col = col + vec3<f32>(0.02, 0.03, 0.06);

    // Transient: a vertical shimmer bloom — bright streaks that sweep up,
    // strongest where the bands already are.
    let streak = abs(fract(p.y * 6.0 - t * 2.0) - 0.5);
    let bloom = (0.5 / (streak * 8.0 + 0.5)) * tr;
    col = col + vec3<f32>(0.6, 0.9, 1.0) * bloom * (band1 + band2) * 0.5;

    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}
