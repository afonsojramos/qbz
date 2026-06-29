// LINE BED — GPU waterfall terrain. Original clean-room port of the Tauri
// LinebedPanel (pure CPU Canvas2D — no shader to copy; authored from its
// projection math). 200 horizontal polylines stacked in depth; each line is one
// historical 256-point spectrum (X = frequency, Y = magnitude, Z = age). The
// newest spectrum is at the far edge; the host pushes a new row each spectral
// frame so the ridged surface scrolls toward the viewer.
//
// The 256×200 heights live in an R32Float texture (binding 4), DEPTH-ORDERED:
// row 0 = newest, row 199 = oldest. This vertex shader projects each curve point
// with a LEVELED pitch-only camera (yaw/roll zeroed so the bed sits flat) and
// SUBDIVIDES each band span with Catmull-Rom so the lines read as smooth curves.
//
// Bindings: 0 = Uniforms (resolution in VS, palette in FS), 4 = heights texture.

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
@group(0) @binding(4) var heights_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // Normalized ridge height (0..1), for the subtle music-driven gradient.
    @location(0) intensity: f32,
};

// --- Camera constants (verbatim from LinebedPanel.svelte) --------------------
const LINE_LENGTH: f32 = 9.0;
const WORLD_AMPLITUDE: f32 = 2.4;
const PLANE_HALF_WIDTH: f32 = 1147.5;     // (255*9)/2
const PLANE_HALF_DEPTH: f32 = 1990.0;     // (199*20)/2
const CAM_X: f32 = 26.1;
const CAM_Y: f32 = 1738.6;
const CAM_Z: f32 = 868.8;
const CAMERA_NEAR: f32 = 80.0;
const FOV_DEG: f32 = 45.0;                 // vertical
const NUM_BANDS: i32 = 256;
// Each band-to-band span is subdivided into SUBDIV Catmull-Rom steps so the
// polyline reads as a CURVE, not stair-stepped segments. MUST match
// LINEBED_SUBDIV in shader_underlay.rs (the vertex count).
const SUBDIV: f32 = 6.0;
// Vertical screen position of the projection origin (lower = the bed sits
// HIGHER). Dropped from 0.5 so the bed lifts off the bottom, clears the player
// bar, and uses the dead space up top.
const VCENTER: f32 = 0.30;

fn height_at(line: u32, band: i32) -> f32 {
    let b = clamp(band, 0, NUM_BANDS - 1);
    return textureLoad(heights_tex, vec2<i32>(b, i32(line)), 0).r;
}

// Uniform cubic B-spline through the 4 neighboring bands — C2-continuous (no
// kinks at the band points) and never overshoots; it APPROXIMATES (gently smooths)
// the samples, so the whole line reads as one continuous curve instead of stitched
// segments. This is what kills the residual sawtooth.
fn curve_height(line: u32, band_f: f32) -> f32 {
    let b0 = i32(floor(band_f));
    let t = band_f - f32(b0);
    let p0 = height_at(line, b0 - 1);
    let p1 = height_at(line, b0);
    let p2 = height_at(line, b0 + 1);
    let p3 = height_at(line, b0 + 2);
    let t2 = t * t;
    let t3 = t2 * t;
    let w0 = (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0;
    let w1 = (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0;
    let w2 = (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0;
    let w3 = t3 / 6.0;
    return p0 * w0 + p1 * w1 + p2 * w2 + p3 * w3;
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) line: u32) -> VsOut {
    // Continuous (subdivided) band position along the line, 0..255.
    let band_f = f32(vid) / SUBDIV;
    let h = curve_height(line, band_f);

    // World position. Only Y is audio-driven; X/Z are the fixed lattice.
    let world_x = band_f * LINE_LENGTH - PLANE_HALF_WIDTH;
    let world_y = h * WORLD_AMPLITUDE;
    let depth_factor = f32(line) / 199.0;
    let world_z = -PLANE_HALF_DEPTH + depth_factor * (PLANE_HALF_DEPTH * 2.0);

    // LEVELED view transform: pitch ONLY (Rx). Yaw (Ry) and roll (Rz) are ZEROED
    // (they were the ~1° skew/roll that tilted the bed) so every line is flat.
    let cosX = cos(0.6543);
    let sinX = sin(0.6543);

    let tX = world_x - CAM_X;
    let tY = world_y - CAM_Y;
    let tZ = world_z - CAM_Z;

    // Rz, Ry = identity (leveled); apply Rx (pitch about the X axis).
    let rX = tX;
    let rY = tY * cosX - tZ * sinX;
    let rZ = tY * sinX + tZ * cosX;

    // Near-clip (v1 — no polyline split; the Tauri version breaks the strip at
    // depth<=near, deferred).
    let depth = max(-rZ, CAMERA_NEAR);

    // SAME focal for X and Y (no aspect correction — faithful to Tauri).
    let focal = u.resolution.y * 0.5 / tan(FOV_DEG * 3.14159265 / 360.0);
    let screen_x = u.resolution.x * 0.5 + rX * focal / depth;
    let screen_y = u.resolution.y * VCENTER - rY * focal / depth;

    // Screen px → clip/NDC (flip Y).
    let ndc_x = screen_x / u.resolution.x * 2.0 - 1.0;
    let ndc_y = 1.0 - screen_y / u.resolution.y * 2.0;

    var out: VsOut;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.intensity = clamp(h / 84.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Bottom cutoff — don't render below ~86% of the view, so the bed ends above
    // the player bar instead of spilling to the window bottom (matches Tauri).
    if (in.pos.y > u.resolution.y * 0.86) {
        discard;
    }
    // Height gradient: the bed/valleys take `primary`, the mountain peaks tip
    // toward `accent`, gradually — peaks also brighten.
    let g = clamp(in.intensity * 1.25, 0.0, 1.0);
    let col = mix(u.primary.rgb, u.accent.rgb, g);
    let a = 0.5 + g * 0.4;
    return vec4<f32>(col, a);
}
