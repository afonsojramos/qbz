// SPECTRAL RIBBON — GPU spectrogram (improved, hi-res). Original clean-room port
// of the Tauri SpectralRibbon (paint-as-you-play sonogram): frequency on Y, time
// on X, a purple→orange heatmap of FFT magnitude. The CPU side (shader_underlay.rs)
// keeps a persistent R8 spectrogram texture (512 freq bands wide × N time columns
// tall) and writes one ROW per new spectral frame at the playback-time column;
// the un-played columns stay zero, so the ribbon fills in as the track plays. This
// shader only SAMPLES that texture and colors it (dB + the Spek ramp).
//
// Bindings: 0 = Uniforms (resolution), 2 = sampler, 3 = spectrogram texture. The
// 144-byte Uniforms block matches the other scenes (we only read `resolution`).

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
@group(0) @binding(2) var samp: sampler;
// The persistent spectrogram: width = 512 frequency bands, height = time columns.
@group(0) @binding(3) var spectrogram: texture_2d<f32>;

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

// Spek-like 7-stop purple→magenta→orange ramp (Tauri spekColor), linear RGB.
fn spek_color(x: f32) -> vec3<f32> {
    let t = clamp(x, 0.0, 1.0);
    // stops: 0.00 black, 0.36 deep-blue, 0.60 indigo, 0.78 purple,
    //        0.92 magenta, 0.98 red-orange, 1.00 orange.
    if (t < 0.36) {
        return mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 0.243), t / 0.36);
    } else if (t < 0.60) {
        return mix(vec3<f32>(0.0, 0.0, 0.243), vec3<f32>(0.055, 0.0, 0.392), (t - 0.36) / 0.24);
    } else if (t < 0.78) {
        return mix(vec3<f32>(0.055, 0.0, 0.392), vec3<f32>(0.361, 0.0, 0.439), (t - 0.60) / 0.18);
    } else if (t < 0.92) {
        return mix(vec3<f32>(0.361, 0.0, 0.439), vec3<f32>(0.745, 0.0, 0.282), (t - 0.78) / 0.14);
    } else if (t < 0.98) {
        return mix(vec3<f32>(0.745, 0.0, 0.282), vec3<f32>(0.863, 0.188, 0.0), (t - 0.92) / 0.06);
    } else {
        return mix(vec3<f32>(0.863, 0.188, 0.0), vec3<f32>(0.933, 0.471, 0.125), (t - 0.98) / 0.02);
    }
}

// Plot rectangle in 0..1 screen space (leaves a margin for axes; the bottom band
// clears the song card / time axis). Tunable.
const PLOT_X0: f32 = 0.055;
const PLOT_X1: f32 = 0.970;
// Y is FLIPPED vs the Slint overlay (the shader samples a y-up framebuffer): these
// are 1 - the overlay's 0.780 / 0.070, so the heatmap lands in the SAME rect as
// the green axes — bass at the 0k axis (bottom), Nyquist at the top.
const PLOT_Y0: f32 = 0.220;
const PLOT_Y1: f32 = 0.930;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let bg = vec3<f32>(0.012, 0.027, 0.047);  // #03070c

    // Inside the plot rectangle?
    if (in.uv.x < PLOT_X0 || in.uv.x > PLOT_X1 || in.uv.y < PLOT_Y0 || in.uv.y > PLOT_Y1) {
        return vec4<f32>(bg, 1.0);
    }

    // Plot-local coords. tf = time fraction (left→right), ff = freq fraction.
    // ff puts band 0 (low/bass) at the BOTTOM (the "0k" axis) and band 511
    // (treble/Nyquist) at the TOP — matching the green axis labels and Tauri.
    let tf = (in.uv.x - PLOT_X0) / (PLOT_X1 - PLOT_X0);
    let ff = (in.uv.y - PLOT_Y0) / (PLOT_Y1 - PLOT_Y0);

    // Sample the spectrogram: u = frequency band (0..1 over 512), v = time column.
    // Un-played columns are zero → background (the ribbon paints as you play).
    let amp = textureSample(spectrogram, samp, vec2<f32>(ff, tf)).r;

    // dB + gamma (Tauri): db in [-120,0] → normalized → ^2.15, alpha 10..166/255.
    let db = 20.0 * log(max(1e-6, amp)) / log(10.0);
    let db_norm = clamp((db + 120.0) / 120.0, 0.0, 1.0);
    let toned = pow(db_norm, 2.15);
    let col = spek_color(toned);
    let a = (10.0 + toned * 156.0) / 255.0;

    // Composite the heatmap over the dark background.
    var outc = mix(bg, col, a);

    // Real-time ceiling line: a horizontal line at the highest active frequency
    // (u.energy_hi.y = smoothed peak band fraction, host-set for mode 4), spanning
    // the full plot width, in the axis green.
    let on_line = 1.0 - smoothstep(0.0, 0.006, abs(ff - u.energy_hi.y));
    outc = mix(outc, vec3<f32>(0.373, 0.722, 0.478), on_line * 0.65);

    return vec4<f32>(outc, 1.0);
}
