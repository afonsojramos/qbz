#version 440

// Plasma (immersive scene, mode 1) — FRAGMENT stage. Line-faithful GLSL 440
// port of `fs_main` in crates/qbz-ui/ui/shaders/plasma.wgsl (the MilkDrop-
// class feedback fluid: sample the previous frame at a warped UV, decay,
// inject orbiting ink + beat splats). The feedback history lives in two
// RGBA8 ping-pong textures owned by the C++ PlasmaItem (the reference keeps
// ONE history and copies the frame back — shader_underlay.rs:1003-1025 —
// which is the same recurrence; ping-pong is how you say "sample last frame,
// write this frame" without a surface readback).
//
// Mechanical differences from the WGSL, enumerated:
//   * textureSample(prev_tex, prev_samp, uv) -> texture(prev_tex, uv)
//     (combined image sampler, binding 1).
//   * bitcast<u32>(i32(x)) -> uint(int(x)): int->uint is a modulo-2^32
//     conversion in GLSL, i.e. the same bit pattern the WGSL bitcast keeps.
//   * The uniform block is the FULL 144-byte std140 scene block (spec 01
//     §1) — this scene reads time/resolution/energy_lo/energy_hi.x/beat/
//     level/level_smooth/primary/secondary/accent; the remaining fields
//     arrive zeroed.
//
// QSB-SKIP-GLES100 — the lattice hash is UINT arithmetic (plasma.wgsl:55-67
// records why a float hash draws "wallpaper join" seams); ES 2.0 has no
// uint. The tier gate keeps the scene off that hardware.

layout(std140, binding = 0) uniform buf {
    float time;
    float phase;
    float beat;
    float level;
    vec2 resolution;
    float levelSmooth;
    float transientAmp;
    vec4 energyLo;
    vec4 energyHi;
    vec4 bandsLo;
    vec4 bandsHi;
    vec4 primary;
    vec4 secondary;
    vec4 accent;
} u;

layout(binding = 1) uniform sampler2D prev_tex;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;

// INTEGER lattice hash (bit-exact) — see the WGSL header above.
float hash2(vec2 p) {
    uint h = uint(int(p.x)) * 0x8da6b343u + uint(int(p.y)) * 0xd8163841u;
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    h = h ^ (h >> 15u);
    return float(h >> 8u) * (1.0 / 16777216.0);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    float a = hash2(i);
    float b = hash2(i + vec2(1.0, 0.0));
    float c = hash2(i + vec2(0.0, 1.0));
    float d = hash2(i + vec2(1.0, 1.0));
    vec2 uf = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, uf.x), mix(c, d, uf.x), uf.y);
}

// Divergence-free flow from a scalar potential: curl = (dphi/dy, -dphi/dx).
vec2 curl_noise(vec2 p) {
    float e = 0.05;
    float dx = vnoise(p + vec2(e, 0.0)) - vnoise(p - vec2(e, 0.0));
    float dy = vnoise(p + vec2(0.0, e)) - vnoise(p - vec2(0.0, e));
    return vec2(dy, -dx) / (2.0 * e);
}

// Soft gaussian blob — an ink emitter.
float blob(vec2 p, vec2 c, float radius) {
    vec2 d = p - c;
    return exp(-dot(d, d) / max(radius * radius, 1e-4));
}

void main() {
    float t = u.time;
    float aspect = u.resolution.x / max(u.resolution.y, 1.0);

    float sub = clamp(u.energyLo.x, 0.0, 1.0);
    float bass = clamp(u.energyLo.y, 0.0, 1.0);
    float mids = clamp(u.energyLo.z, 0.0, 1.0);
    float air = clamp(u.energyHi.x, 0.0, 1.0);
    float beat = clamp(u.beat, 0.0, 1.0);
    float level = clamp(u.level, 0.0, 1.0);
    float lvl = clamp(u.levelSmooth, 0.0, 1.0);

    // Centered, aspect-correct coords.
    vec2 c = (uv - vec2(0.5)) * vec2(aspect, 1.0);

    float pres = clamp(u.energyLo.w, 0.0, 1.0);

    // === Feedback advection — turbulent, music-cranked. ===
    // Bass inhale/zoom (stronger) + a beat kick.
    float zoom = 1.0 + 0.006 + (sub + bass) * 0.03 + beat * 0.02;
    // Mids swirl harder and faster; beats jolt the rotation.
    float ang = (0.006 + mids * 0.05) * sin(t * 0.5 + length(c) * 4.0) + beat * 0.035;
    float ca = cos(ang);
    float sa = sin(ang);
    vec2 w = c * zoom;
    w = vec2(w.x * ca - w.y * sa, w.x * sa + w.y * ca);
    // Two curl-noise octaves (treble + presence) -> filaments + fine turbulence.
    w += curl_noise(c * 2.5 + vec2(t * 0.12, -t * 0.10)) * (0.003 + air * 0.015);
    w += curl_noise(c * 5.5 - vec2(t * 0.09, t * 0.08)) * (0.0015 + pres * 0.009);
    vec2 warpUv = w / vec2(aspect, 1.0) + vec2(0.5);

    vec3 field = texture(prev_tex, warpUv).rgb;

    // Decay: faster so it stays ALIVE (not muddy); louder lingers a bit longer.
    field *= 0.85 + lvl * 0.06;

    // === FOUR emitters, fast orbits — more elements, more motion. ===
    vec2 e1 = vec2(sin(t * 0.5), cos(t * 0.6)) * (0.25 + bass * 0.2);
    vec2 e2 = vec2(sin(-t * 0.42 + 2.1), cos(-t * 0.38 + 1.3)) * (0.3 + mids * 0.18);
    vec2 e3 = vec2(sin(t * 0.74 + 1.0), cos(-t * 0.66 + 3.0)) * (0.22 + pres * 0.2);
    vec2 e4 = vec2(cos(t * 0.33 + 4.0), sin(t * 0.58 + 0.5)) * (0.34 + air * 0.16);
    field += u.primary.rgb * blob(c, e1, 0.045 + bass * 0.06) * (0.14 + level * 0.8);
    field += u.secondary.rgb * blob(c, e2, 0.05 + mids * 0.05) * (0.14 + level * 0.65);
    field += u.accent.rgb * blob(c, e3, 0.04 + pres * 0.05) * (0.10 + level * 0.6);
    field += u.primary.rgb * blob(c, e4, 0.035 + air * 0.04) * (0.07 + air * 0.6);

    // === Beat splats — TWO, bigger and brighter, detonating on the onset. ===
    vec2 sp1 = vec2(sin(t * 0.9) * 0.35, cos(t * 0.7) * 0.28);
    vec2 sp2 = vec2(cos(t * 1.1 + 2.0) * 0.3, sin(t * 0.85 + 1.0) * 0.32);
    field += u.accent.rgb * blob(c, sp1, 0.05 + beat * 0.14) * beat * 2.2;
    field += u.secondary.rgb * blob(c, sp2, 0.04 + beat * 0.10) * beat * 1.6;

    // Treble shimmer on the crests.
    float luma = dot(field, vec3(0.33, 0.34, 0.33));
    field += u.accent.rgb * air * smoothstep(0.4, 0.9, luma) * 0.18;

    field = clamp(field, vec3(0.0), vec3(1.3));
    fragColor = vec4(field, 1.0);
}
