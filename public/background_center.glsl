#version 300 es
precision highp float;

uniform float u_time;
uniform float u_loading_time;
uniform vec2 u_resolution;

const float k_loading_speed_mult = 10.0;
const float k_normal_speed_mult = 1.0;
const float k_loading_transition_seconds = 0.5;

out vec4 outColor;

void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution.xy;
    uv = uv * 2.0 - 1.0;
    uv.x *= u_resolution.x / u_resolution.y;

    // --- 1. Background ---
    vec3 bg_color = vec3(0.0);

    float bgWave = sin((uv.x + uv.y) * 24.0 + u_time * 0.5);
    float bgDiag = step(0.4, bgWave);
    bg_color += vec3(0.02, 0.03, 0.08) * bgDiag;

    // --- Distance Metric ---
    // 0.866025 is approx sqrt(3)/2, creating a pointy-topped hexagon.
    vec2 p = abs(uv);
    float hex_dist = max(p.x, p.x * 0.5 + p.y * 0.866025);
    float circle_dist = length(uv);

    // --- 2. Glowing Center (Circular) ---
    // Increased the multiplier from 6.0 to 8.5 to pull the glow inwards (darker overall)
    float core_glow = exp(-circle_dist * 5.0);
    float core_ring = smoothstep(0.02, 0.0, abs(hex_dist - 0.15)) * 0.5;

    // Lowered the base color intensity from (0.2, 0.6, 1.0) to make it darker
    vec3 center_color = vec3(0.06, 0.25, 0.4) * (core_glow + core_ring);

    // --- 3. Circling Lines (Orbitals) ---
    float angle = atan(uv.y, uv.x);
    vec3 orbital_color = vec3(0.0);
    float transition_seconds = max(k_loading_transition_seconds, 0.0001);
    float loading_elapsed = u_time - u_loading_time;
    float loading_progress = (u_loading_time == 0.0)
        ? 0.0 : clamp(loading_elapsed / transition_seconds, 0.0, 1.0);
    float loading_ease = loading_progress * loading_progress * (3.0 - 2.0 * loading_progress);

    float phase_time = u_time * k_loading_speed_mult;
    if (u_loading_time != 0.0) {
        float t0 = u_loading_time;
        float s = loading_progress;
        float e_int = s * s * s - 0.5 * s * s * s * s;
        float transition_integral = transition_seconds *
                (k_loading_speed_mult * s + (k_normal_speed_mult - k_loading_speed_mult) * e_int);
        float after = max(u_time - t0 - transition_seconds, 0.0);
        phase_time = k_loading_speed_mult * t0 + transition_integral + k_normal_speed_mult * after;
    }

    for (float i = 1.0; i <= 4.0; i++) {
        float outer_r = 0.25 + i * 0.12;
        float inner_r = 0.08 + i * 0.05;
        float r = mix(inner_r, outer_r, loading_ease);

        float base_speed = (mod(i, 2.0) == 0.0 ? 1.0 : -1.0) * (0.3 + i * 0.15);
        float segments = 2.0 + i;
        float current_angle = angle * segments + phase_time * base_speed;

        float ring_width = 0.003 + 0.001 * i;

        // We use hex_dist here so the orbitals are also hexagons
        float ring_mask = smoothstep(ring_width + 0.01, ring_width, abs(hex_dist - r));
        float dash = smoothstep(0.1, 0.5, sin(current_angle));
        float head = smoothstep(0.9, 0.98, fract(current_angle / 6.28318));

        vec3 col = 0.5 + 0.5 * cos(u_time * 0.4 + i + vec3(0.0, 1.5, 3.0));
        orbital_color += ring_mask * dash * col * (0.8 + head * 2.5);
    }

    vec3 final_color = vec3(0.03, 0.03, 0.05) + bg_color + center_color + orbital_color;

    outColor = vec4(final_color, 1.0);
}
