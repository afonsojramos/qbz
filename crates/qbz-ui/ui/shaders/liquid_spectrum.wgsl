// LIQUID SPECTRUM — circular "liquid" audio-spectrum ring (Motion-Array style).
//
// The FFT bands are wrapped around a circle (angle = frequency, radius pushed out
// by magnitude) and smoothed into an organic liquid blob, glowing in the album
// palette, with a dark center disc (where album art/logo would sit), a rotating
// radial-rays backdrop, and a beat-triggered radial burst. One fullscreen-triangle
// pass; reads the enriched uniform pack (bands, beat, level, palette).
//
// CPU side: src/shader_underlay.rs (mode 6). Uniforms block byte-identical to the
// `Uniforms` #[repr(C)] struct (std140, 144 bytes). ENTIRELY original code.

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

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318530;

fn band_at(i: u32) -> f32 {
    var b = array<f32, 8>(
        u.bands_lo.x, u.bands_lo.y, u.bands_lo.z, u.bands_lo.w,
        u.bands_hi.x, u.bands_hi.y, u.bands_hi.z, u.bands_hi.w,
    );
    return b[i & 7u];
}

// Smooth (B-spline-ish smoothstep) sample of the 8 bands at a fractional index,
// so the ring reads liquid instead of 8 discrete lobes.
fn bands_smooth(pos: f32) -> f32 {
    let i0 = i32(floor(pos));
    let f = pos - f32(i0);
    let a = band_at(u32(i0 & 7));
    let b = band_at(u32((i0 + 1) & 7));
    return mix(a, b, smoothstep(0.0, 1.0, f));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let res = u.resolution;
    let aspect = res.x / max(res.y, 1.0);
    let p = (in.uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(aspect, 1.0);
    let r = length(p);
    let ang = atan2(p.y, p.x);              // -PI..PI

    let bass = clamp(u.energy_lo.y, 0.0, 1.0);
    let mids = clamp(u.energy_lo.z, 0.0, 1.0);
    let air = clamp(u.energy_hi.x, 0.0, 1.0);
    let beat = clamp(u.beat, 0.0, 1.0);
    let level = clamp(u.level, 0.0, 1.0);

    // Angle → band, MIRRORED so the ring is symmetric (classic look), smoothed.
    let a01 = ang / TAU + 0.5;              // 0..1 around the circle
    let folded = abs(a01 * 2.0 - 1.0);      // 0..1 mirrored across the vertical
    let mag = bands_smooth(folded * 7.0);

    // Liquid ring: base radius + spectrum push-out + an organic low-freq wobble.
    let baseR = 0.20 + beat * 0.025;
    let wobble = sin(ang * 3.0 + u.time * 1.1) * 0.012 * (0.4 + mids)
        + sin(ang * 7.0 - u.time * 0.8) * 0.006 * (0.3 + air);
    let ringR = baseR + mag * 0.18 + bass * 0.03 + wobble;
    let d = abs(r - ringR);
    let thick = 0.010 + mag * 0.018;
    let core = smoothstep(thick, 0.0, d);           // crisp ring line
    let glow = thick / (d + thick * 0.9) * 0.55;    // neon glow falloff

    // Palette gradient around the ring; peaks tip toward accent.
    let hue = fract(a01 + u.time * 0.04);
    var ringCol = mix(u.primary.rgb, u.secondary.rgb, hue);
    ringCol = mix(ringCol, u.accent.rgb, clamp(mag * 1.2, 0.0, 1.0));

    // Rotating radial-rays backdrop, dark and dim, fading out from center.
    let rays = pow(0.5 + 0.5 * sin(ang * 12.0 + u.time * 0.3), 3.0);
    let bgFade = smoothstep(0.85, 0.15, r);
    var col = u.secondary.rgb * rays * bgFade * (0.10 + level * 0.28);

    // Beat burst: a radial flash of accent just outside the ring.
    let burst = beat * pow(rays, 2.0)
        * smoothstep(ringR, ringR + 0.06, r)
        * (1.0 - smoothstep(ringR + 0.06, 0.95, r));
    col += u.accent.rgb * burst * 0.9;

    // The ring itself (line + glow), brightened on the beat.
    col += ringCol * (core + glow) * (0.9 + beat * 0.6);

    // Dark center disc (album art / logo would composite here later).
    let inner = smoothstep(baseR - 0.005, baseR - 0.03, r);
    col *= 1.0 - inner * 0.7;

    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.25));
    return vec4<f32>(col, 1.0);
}
