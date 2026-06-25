// NEON TUNNEL — original clean-room fragment shader (Checkpoint E).
//
// A polar-warped infinite tunnel: concentric neon rings rush toward the
// viewer along `time`, the apparent zoom/speed pulses with the sub-bass
// energy (u.energy0), and a transient (u.transient) flares the ring edges
// to a bright white. One fullscreen-triangle pass, no loops — GPU-cheap.
//
// CPU side: src/shader_underlay.rs. The Uniforms block here is byte-identical
// to the `Uniforms` #[repr(C)] struct there (vec4-aligned, 32 bytes). This is
// ENTIRELY original code (no external source copied) per the Flathub license
// rule.

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

// Smooth 1->0 falloff used to shape the neon ring lines.
fn ring_glow(x: f32, width: f32) -> f32 {
    return width / (abs(x) + width);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = u.time;
    let bass = clamp(u.energy0, 0.0, 1.0);
    let tr = clamp(u.transient, 0.0, 1.0);

    // Aspect-corrected centered coordinates, range roughly [-1, 1] vertically.
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    var p = (in.uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(2.0 * aspect, 2.0);

    // Polar warp: radius -> tunnel depth, angle -> tunnel wall coordinate.
    let r = length(p) + 0.0001;
    let a = atan2(p.y, p.x);

    // Depth coordinate. 1/r maps the screen edges to the near mouth of the
    // tunnel and the center to infinity; bass speeds up the rush, transient
    // gives a brief forward lurch.
    let speed = 0.5 + bass * 1.1 + tr * 0.6;
    let depth = 1.0 / r + t * speed;

    // Concentric rings advancing in depth. fract() makes them repeat; the
    // distance to the nearest ring center (0.5) drives the neon line.
    let ring_d = abs(fract(depth) - 0.5);
    let line_w = 0.04 + bass * 0.05;
    let rings = ring_glow(ring_d, line_w);

    // Angular flutes running down the tunnel walls — a few sine lobes around
    // the circle, slowly twisting with depth so the walls feel like they move.
    let flutes = 0.5 + 0.5 * sin(a * 8.0 + depth * 1.5 + t * 0.4);

    // Neon hue cycles with depth so rings recede through the spectrum. We build
    // colour from three phase-shifted sines (a compact HSV-ish ramp).
    let hue = depth * 0.35 + t * 0.15 + bass * 0.4;
    let pi = 3.14159265;
    let cr = sin(hue * pi + 0.0) * 0.5 + 0.5;
    let cg = sin(hue * pi + 2.094) * 0.5 + 0.5;
    let cb = sin(hue * pi + 4.188) * 0.5 + 0.5;
    var neon = vec3<f32>(cr, cg, cb);

    // Compose: ring brightness modulated by the flute pattern, lifted by bass.
    let intensity = rings * (0.55 + flutes * 0.55) * (0.7 + bass * 0.9);
    var col = neon * intensity;

    // Center vignette so the far end of the tunnel fades to black (and never
    // blows out at r -> 0 where 1/r explodes).
    let vignette = smoothstep(0.0, 0.35, r);
    col = col * vignette;

    // A transient flares the ring edges to white for a punchy beat hit.
    col = col + vec3<f32>(rings * tr * 0.9 * vignette);

    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}
