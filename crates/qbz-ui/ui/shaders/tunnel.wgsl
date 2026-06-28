// DOOM TUNNEL — original clean-room fragment shader.
//
// A FAST forward-flight RECTANGULAR corridor: nested receding square frames
// (box / Chebyshev distance, NOT length() — that is what made the old port a
// circle), radial speed-lines streaming out of the vanishing point, four
// converging corner lines, a dark square portal at the vanishing point, and a
// WINDING centerline — the curvature lives in the PATH (the corridor snakes as
// it recedes), NOT in the cross-section (the rectangle never deforms; it only
// shifts). Driven by the FFT: the 8 log bands sweep per-ring, bass = thrust,
// beat = punch, and the host forward-motion clock is amplified for real speed;
// colors come from the album-art palette. One fullscreen-triangle pass, no loops.
//
// CPU side: src/shader_underlay.rs. The Uniforms block is byte-identical to the
// `Uniforms` #[repr(C)] struct there (std140, 144 bytes). ENTIRELY original code
// (no external source copied) per the Flathub license rule.

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
    // Classic oversized fullscreen triangle — no vertex buffer.
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

// One of the 8 log FFT bands, selected by ring index (per-ring spectral sweep).
fn band_at(i: u32) -> f32 {
    var b = array<f32, 8>(
        u.bands_lo.x, u.bands_lo.y, u.bands_lo.z, u.bands_lo.w,
        u.bands_hi.x, u.bands_hi.y, u.bands_hi.z, u.bands_hi.w,
    );
    return b[i & 7u];
}

// --- Tunables (easy to dial) -------------------------------------------------
// Forward-flight multiplier on the host phase clock. The clock wraps at 4096
// (an integer multiple of 8), so any INTEGER multiplier keeps floor()/fract()/
// `& 7u` ring math seamless across the (≈hourly) wrap.
const FLIGHT_SPEED: f32 = 6.0;
// How hard the centerline snakes. The bend is applied to the PATH, so the
// rectangles only translate — they never warp.
const BEND_AMT: f32 = 0.14;
// Number of radial speed-lines fanning out of the vanishing point (keep it an
// even integer so they stay continuous across the atan2 seam at ±π).
const SPOKES: f32 = 16.0;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = u.time;

    // FFT aggregates from the 8 log bands (match the Tauri canvas crossovers:
    // bars 0-3 / 4-9 / 10-15 land on even band-pair boundaries).
    let bass = clamp((u.bands_lo.x + u.bands_lo.y) * 0.5, 0.0, 1.0);
    let mid = clamp((u.bands_lo.z + u.bands_lo.w + u.bands_hi.x) / 3.0, 0.0, 1.0);
    let high = clamp((u.bands_hi.y + u.bands_hi.z + u.bands_hi.w) / 3.0, 0.0, 1.0);
    let beat = clamp(u.beat, 0.0, 1.0);

    // FAST flight: the host forward-motion clock (base 0.012 + level + beat per
    // frame, wrapped at 4096) amplified. Because 4096 * FLIGHT_SPEED is an
    // integer multiple of 8, floor()/fract()/`& 7u` stay continuous at the wrap,
    // and the amplified `level`/`beat` terms make the speed itself music-driven.
    let flight = u.phase * FLIGHT_SPEED;

    // Corridor cross-section is WIDER than tall (a hallway, not a square shaft):
    // wider-than-tall scale derived from the viewport aspect.
    let viewAspect = u.resolution.x / max(u.resolution.y, 1.0);
    let scaleX = clamp(1.04 + viewAspect * 0.25, 1.22, 1.58);
    let scaleY = clamp(1.38 - scaleX * 0.45, 0.62, 0.82);

    // Vanishing point: a gentle Lissajous sway (from `time` only, so its own
    // phase never jumps). Kept small — the real motion is the depth-wind below.
    let cphase = t * 0.18;
    var center = vec2<f32>(0.5, 0.5);
    center.x += sin(cphase) * 0.03 + sin(cphase * 2.08 + 1.2) * 0.012;
    center.y += cos(cphase * 0.82 + 0.7) * 0.02;

    // First pass: unbent cross-section + depth, so we know how DEEP this pixel
    // sits before bending the path.
    let p0 = (in.uv - center) / vec2<f32>(scaleX, scaleY);
    let r0 = max(abs(p0.x), abs(p0.y)) + 1e-4;
    let depth0 = 1.0 / r0;
    let z0 = log2(depth0) - flight;

    // WINDING CENTERLINE — the curvature is in the PATH, not the rectangle. A
    // lateral wave ALONG the depth axis (z0): the corridor snakes left/right and
    // up/down as it recedes, and the wave scrolls toward you with `flight`, so
    // it feels like driving a curving road. The near mouth barely bends; deep
    // sections swing the most. Louder music = a livelier road. We translate the
    // sampling point — the square cross-section is preserved exactly.
    let windE = 0.75 + u.level_smooth * 0.6 + beat * 0.3;
    let windX = sin(z0 * 0.65 + t * 0.6) + 0.5 * sin(z0 * 1.27 - t * 0.9);
    let windY = 0.6 * cos(z0 * 0.85 + t * 0.5) + 0.35 * sin(z0 * 1.6 + t * 0.7);
    let bendDepth = smoothstep(0.0, 2.6, depth0);   // ~0 at the mouth -> 1 deep in
    let bend = vec2<f32>(windX, windY) * (bendDepth * BEND_AMT * windE);

    // Second pass: BENT cross-section coords; BOX distance (Chebyshev) → SQUARE
    // rings that ride the winding path.
    let p = p0 - bend;
    let r = max(abs(p.x), abs(p.y)) + 1e-4;
    let depth = 1.0 / r;
    let z = log2(depth) - flight;             // minus → rings grow outward (forward)
    let ringId = i32(floor(z));
    let ringFrac = fract(z);

    // Rectangle outline: bright on the frame edge, dark in the gap. Thickness
    // breathes with bass + beat.
    let lineW = 0.06 + bass * 0.16 + beat * 0.10;
    let edge = smoothstep(0.0, lineW, ringFrac) * smoothstep(0.0, lineW, 1.0 - ringFrac);
    let frame = 1.0 - edge;

    // Per-ring spectral pulse — the band sweeping bass→treble into depth.
    let ringPulse = band_at(u32(ringId & 7));

    // Four corridor corner lines (|p.x| == |p.y|) converging to the portal — the
    // strongest "hallway" cue.
    let corner = 1.0 - smoothstep(0.0, 0.05, abs(abs(p.x) - abs(p.y)));

    // RADIAL SPEED-LINES fanning OUT of the vanishing point: thin spokes at
    // `SPOKES` angles, invisible at the portal, fading in just outside it and
    // streaming toward the mouth. They rotate very slowly and shimmer hard with
    // treble + the beat, so they read as light rushing past.
    let ang = atan2(p.y, p.x);
    let spokeWave = 0.5 + 0.5 * cos(ang * SPOKES + t * 0.25);
    let spoke = pow(spokeWave, 12.0)
        * smoothstep(0.02, 0.14, r)            // emanate from the vanishing point
        * (1.0 - smoothstep(0.85, 1.3, r));    // ease off at the near mouth

    // Depth shading: black square portal at the center, lit corridor outward.
    let depthShade = smoothstep(0.04, 0.55, r);
    let nearWeight = smoothstep(0.35, 1.1, r);

    // Palette: rings recede primary (near mouth) → secondary (mid) → accent (far).
    let palT = 1.0 - smoothstep(0.05, 0.6, r);
    var ringCol = mix(u.primary.rgb, u.secondary.rgb, smoothstep(0.0, 0.5, palT));
    ringCol = mix(ringCol, u.accent.rgb, smoothstep(0.5, 1.0, palT));

    // Walls: lit gray gradient distinguishing left/right from top/bottom walls.
    let wallLit = select(0.10, 0.16, abs(p.x) > abs(p.y));
    var col = vec3<f32>(wallLit) * depthShade * (0.6 + mid * 0.8);

    // Ring frames (palette + spectral pulse + bass) and the corner lines.
    let ringBright = frame * depthShade * (0.45 + ringPulse * 1.3 + bass * 0.5);
    col += ringCol * ringBright;
    col += u.accent.rgb * corner * depthShade * (0.35 + high * 0.7) * frame;

    // Radial speed-lines (toward `primary` so they read as bright streaks).
    col += u.primary.rgb * spoke * (0.5 + high * 1.4 + beat * 0.8);

    // Beat punch + a treble rim sparkle on the nearest ring (toward accent).
    col *= 1.0 + beat * 0.6;
    col += u.accent.rgb * high * nearWeight * frame * 0.25;

    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}
