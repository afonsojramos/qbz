// AURORA WARP — original clean-room fragment shader.
//
// Flowing aurora curtains driven by domain-warped sine fields. Three curtains
// take their colors from the album-art palette (primary / secondary / accent);
// bass swells the low curtain, mids sway the body, treble shimmers the crests
// (cross-fading each curtain toward accent), beats kick the brightness and lift
// the curtains, and the smoothed level sets the baseline bloom. One fullscreen-
// triangle pass, fixed work per pixel — GPU-cheap, no loops.
//
// CPU side: src/shader_underlay.rs. The Uniforms block is byte-identical to the
// `Uniforms` #[repr(C)] struct (std140, 144 bytes). ENTIRELY original code (no
// external source copied) per the Flathub license rule.

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

// A small smooth flow field: cheap sine lobes that act like a fake curl, so the
// curtains ripple horizontally instead of sliding rigidly.
fn flow(p: vec2<f32>, t: f32) -> f32 {
    let a = sin(p.x * 2.3 + t * 0.7) * 0.5;
    let b = sin(p.x * 4.7 - t * 0.45 + p.y * 1.3) * 0.25;
    let c = sin((p.x + p.y) * 1.7 + t * 0.9) * 0.18;
    return a + b + c;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = u.time;
    let bass = clamp(u.energy_lo.y, 0.0, 1.0);
    let mids = clamp(u.energy_lo.z, 0.0, 1.0);
    let presence = clamp(u.energy_lo.w, 0.0, 1.0);
    let air = clamp(u.energy_hi.x, 0.0, 1.0);
    let beat = clamp(u.beat, 0.0, 1.0);
    let lvl = clamp(u.level_smooth, 0.0, 1.0);

    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    // x spread by aspect so the curtains aren't squashed on wide windows.
    var p = vec2<f32>((in.uv.x - 0.5) * aspect, in.uv.y);

    // Mids drive the sway amplitude & speed.
    let swaySpeed = 0.6 + mids * 0.9;
    let warp1 = flow(p * 3.0, t * swaySpeed);
    let warp2 = flow(p * 2.0 + vec2<f32>(7.3, 0.0), t * (0.8 * swaySpeed) + 2.0);
    let warp3 = flow(p * 2.6 + vec2<f32>(-4.1, 1.0), t * (0.65 * swaySpeed) + 4.0);

    // Curtain widths: bass swells the low curtain; beat lifts; presence the top.
    let bw1 = 0.10 + bass * 0.16 + beat * 0.03;
    let bw2 = 0.09 + mids * 0.10;
    let bw3 = 0.07 + presence * 0.10;

    // Anchors drift on a slow vertical LFO + a beat lift; sway from warp + mids.
    let lift = beat * 0.03;
    let sway = 0.08 + mids * 0.12;
    let center1 = 0.40 + 0.02 * sin(t * 0.23) + warp1 * sway - lift;
    let center2 = 0.60 + 0.02 * sin(t * 0.19 + 1.7) + warp2 * sway - lift;
    let center3 = 0.50 + 0.025 * sin(t * 0.16 + 3.1) + warp3 * (sway * 0.9) - lift;

    let band1 = bw1 / (abs(p.y - center1) + bw1);
    let band2 = bw2 / (abs(p.y - center2) + bw2);
    let band3 = bw3 / (abs(p.y - center3) + bw3);

    // Treble shimmer cross-fades each curtain base color toward accent.
    let shFreq = 5.0 + air * 8.0;
    let sh1 = 0.5 + 0.5 * sin(p.x * shFreq + t * 1.3);
    let sh2 = 0.5 + 0.5 * sin(p.x * (shFreq * 0.8) - t * 1.1 + 1.0);
    let sh3 = 0.5 + 0.5 * sin(p.x * (shFreq * 1.2) + t * 1.6 + 2.0);
    let shGain = 0.25 + air * 0.5;

    let col1 = mix(u.primary.rgb, u.accent.rgb, sh1 * shGain);
    let col2 = mix(u.secondary.rgb, u.accent.rgb, sh2 * shGain);
    let col3 = mix(u.accent.rgb, u.primary.rgb, sh3 * shGain);

    var col = col1 * band1 * (0.55 + sh1 * 0.45);
    col += col2 * band2 * (0.55 + sh2 * 0.45);
    col += col3 * band3 * (0.5 + sh3 * 0.4) * (0.4 + presence * 0.8);

    // Baseline brightness from the smoothed level + beat; dim primary floor glow.
    col *= 0.5 + lvl * 0.9;
    col *= 1.0 + beat * 0.5;
    col += u.primary.rgb * 0.04;

    // Beat bloom: vertical shimmer streaks where the curtains already are.
    let streak = abs(fract(p.y * 6.0 - t * 2.0) - 0.5);
    let bloom = (0.5 / (streak * 8.0 + 0.5)) * beat;
    col += u.accent.rgb * bloom * (band1 + band2 + band3) * 0.4;

    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}
