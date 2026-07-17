// Ambient — the calm, low-energy album-colored scene for the APP-WIDE dynamic
// background ("modo Cider"). Unlike plasma/tunnel/aurora this is deliberately
// NOT audio-reactive on the fast drivers (no `beat`/`transient`): it is a slow
// mesh-gradient of the album triad (primary/secondary/accent) drifting on
// long-period sinusoids, with only a gentle breathe from `level_smooth`. It is
// meant to sit behind the entire shell for minutes at a time without ever
// pulling the eye, so text over the translucent surfaces stays readable (the
// Slint layer adds the dark scrim; this scene stays mid-to-low brightness).
//
// Uses ONLY binding 0 (uniforms) — a scene that declares a SUBSET of the shared
// bind-group layout is valid (see shader_underlay.rs build_shared). Registered
// as mode 7, index [5] in SHADER_SOURCES.

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

// Cheap smooth value noise (hash + bilinear), a couple of octaves. No loops over
// large ranges — this is a background that must stay near-free on an integrated
// GPU.
fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    let w = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.55;
    var q = p;
    v = v + amp * vnoise(q); q = q * 2.02; amp = amp * 0.5;
    v = v + amp * vnoise(q); q = q * 2.03; amp = amp * 0.5;
    v = v + amp * vnoise(q);
    return v;
}

// Soft radial weight for a color blob centered at `c`.
fn blob(uv: vec2<f32>, c: vec2<f32>, r: f32) -> f32 {
    let d = distance(uv, c);
    return exp(-(d * d) / (r * r));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Aspect-correct so the drift looks even on wide windows.
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    var uv = in.uv;
    uv.x = uv.x * aspect;

    // Clock. FAST enough that the flow is clearly visible in seconds (blob
    // orbits are ~10-18s) — a subtle drift reads as "not moving". `level_smooth`
    // adds a gentle breathe on top when audio is flowing.
    let t = u.time * 0.5;
    let breathe = 1.0 + 0.10 * u.level_smooth;

    // Stronger domain warp so the blobs flow like fluid, not on rigid circles.
    let warp = vec2<f32>(
        fbm(uv * 1.8 + vec2<f32>(t * 0.6, t * 0.2)),
        fbm(uv * 1.8 + vec2<f32>(-t * 0.25, t * 0.55)),
    );
    let p = uv + (warp - 0.5) * 0.55;

    // Three album-colored blobs sweeping across most of the screen (big travel
    // so the motion is obvious), each on its own period so they never lock.
    let cA = vec2<f32>((0.35 + 0.34 * sin(t * 0.41)) * aspect,
                       0.42 + 0.30 * cos(t * 0.33));
    let cB = vec2<f32>((0.66 + 0.32 * sin(t * 0.37 + 2.1)) * aspect,
                       0.58 + 0.34 * cos(t * 0.29 + 1.3));
    let cC = vec2<f32>((0.50 + 0.36 * cos(t * 0.31 + 4.0)) * aspect,
                       0.34 + 0.30 * sin(t * 0.44 + 3.2));

    let r = 0.62 * breathe;
    let wA = blob(p, cA, r);
    let wB = blob(p, cB, r * 0.92);
    let wC = blob(p, cC, r * 0.85);
    let wSum = wA + wB + wC + 0.0001;

    var col = (u.primary.rgb * wA + u.secondary.rgb * wB + u.accent.rgb * wC) / wSum;

    // Push saturation/contrast a touch so the album hues read as a living
    // gradient rather than a muddy average.
    let luma = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    col = mix(vec3<f32>(luma), col, 1.25);

    // Vertical falloff → a touch darker at the very top/bottom edges so chrome
    // (titlebar, player bar) sits on calmer color. Kept subtle.
    let vshade = 1.0 - 0.16 * pow(abs(in.uv.y - 0.5) * 2.0, 2.0);
    col = col * vshade;

    // Overall brightness: brighter than before — the content area now shows the
    // full background, and the Slint scrim (QBZ_BG_DIM) provides the legibility
    // dim, so the base can stay vivid without glaring.
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * 0.82;

    // A whisper of grain to avoid banding on the smooth gradient.
    let grain = (vnoise(in.uv * u.resolution * 0.5) - 0.5) * 0.015;
    col = col + vec3<f32>(grain);

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
