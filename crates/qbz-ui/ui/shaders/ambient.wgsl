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
// Sin-FREE hash (Dave Hoskins). The old `fract(sin(dot(p,k))*43758)` breaks on
// some GPUs (NVIDIA): as the noise coords grow with time, sin's argument gets
// huge and the driver's range-reduction precision diverges from Intel's,
// degenerating the noise into big blocks. This integer-style fract hash is
// stable across GPUs regardless of coordinate magnitude.
fn hash2(p: vec2<f32>) -> f32 {
    var p3 = fract(p.xyx * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
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

// Metaball potential (r²/d²): summed over several movers, the iso-surface merges
// and splits organically — the "amoeba / lava-lamp" morph. Unlike a gaussian
// blob this has a long tail, so nearby balls fuse into stretched shapes.
fn mball(uv: vec2<f32>, c: vec2<f32>, r: f32) -> f32 {
    let d = uv - c;
    return (r * r) / (dot(d, d) + 0.0009);
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
    let t = u.time * 0.75;
    let breathe = 1.0 + 0.12 * u.level_smooth;

    // Two-octave domain warp. The LOW octave does the big organic flow; the high
    // octave is kept small + lower-frequency — a strong high-freq warp folds the
    // metaball field into sharp creases that read as ugly hard edges over the
    // translucent UI, so it's gentle here.
    let w1 = vec2<f32>(
        fbm(uv * 1.3 + vec2<f32>(t * 0.5, t * 0.2)),
        fbm(uv * 1.3 + vec2<f32>(-t * 0.3, t * 0.45)),
    );
    let w2 = vec2<f32>(
        fbm(uv * 2.3 + vec2<f32>(-t * 0.6, t * 0.45)),
        fbm(uv * 2.3 + vec2<f32>(t * 0.55, -t * 0.35)),
    );
    let p = uv + (w1 - 0.5) * 0.68 + (w2 - 0.5) * 0.14;

    // Four album-colored METABALLS on big wandering orbits. Their r²/d² fields
    // sum, so where two get close they FUSE into a stretched amoeba lobe, and
    // pull apart as they separate — the morphing Cider/lava-lamp motion.
    let c4 = mix(u.primary.rgb, u.accent.rgb, 0.5);
    let cA = vec2<f32>((0.32 + 0.30 * sin(t * 0.40)) * aspect,       0.42 + 0.30 * cos(t * 0.33));
    let cB = vec2<f32>((0.66 + 0.30 * sin(t * 0.35 + 2.1)) * aspect, 0.56 + 0.32 * cos(t * 0.29 + 1.3));
    let cC = vec2<f32>((0.50 + 0.34 * cos(t * 0.31 + 4.0)) * aspect, 0.36 + 0.30 * sin(t * 0.45 + 3.2));
    let cD = vec2<f32>((0.46 + 0.32 * sin(t * 0.27 + 5.3)) * aspect, 0.64 + 0.28 * cos(t * 0.49 + 0.7));

    let rr = 0.34 * breathe;
    let fA = mball(p, cA, rr);
    let fB = mball(p, cB, rr * 0.95);
    let fC = mball(p, cC, rr * 0.88);
    let fD = mball(p, cD, rr * 0.82);
    let field = fA + fB + fC + fD;

    // Metaball-weighted album color (which lobe dominates here).
    var col = (u.primary.rgb * fA + u.secondary.rgb * fB + u.accent.rgb * fC + c4 * fD)
        / (field + 0.0001);

    // The AMOEBA structure: the iso-surface. A WIDE smoothstep so the lobes melt
    // into the base with a long soft gradient instead of a hard rim (the ugly
    // edges), and a gentler bright/dark spread so transitions never snap.
    let shape = smoothstep(0.45, 3.4, field);
    col = col * mix(0.42, 1.12, shape);

    // Push saturation/contrast a little so the lobes read as album color, not a
    // muddy average — but not so hard it re-sharpens the transitions.
    let luma = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    col = mix(vec3<f32>(luma), col, 1.28);

    // Vertical falloff → a touch darker at the very top/bottom edges so chrome
    // (titlebar, player bar) sits on calmer color. Kept subtle.
    let vshade = 1.0 - 0.16 * pow(abs(in.uv.y - 0.5) * 2.0, 2.0);
    col = col * vshade;

    // Overall brightness — vivid; the Slint scrim (QBZ_BG_DIM) provides the
    // legibility dim, so the base can stay bright without glaring.
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * 0.92;

    // A touch of grain (dither) to break up 8-bit banding on the smooth gradient
    // — banding rings read as faint hard edges too.
    let grain = (vnoise(in.uv * u.resolution * 0.5) - 0.5) * 0.022;
    col = col + vec3<f32>(grain);

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
