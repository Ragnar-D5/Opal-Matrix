#version 300 es
precision highp float;

uniform float u_time;
uniform float u_last_changed_time;
uniform float u_state;
uniform float u_prev_state;
uniform vec2 u_resolution;

const float k_loading_speed_mult = 15.0;
const float k_normal_speed_mult = 1.0;
const float k_state_transition_seconds = 0.5;
const float k_state_count = 4.0;
const float k_state_max = k_state_count - 1.0;
const float k_speed_drop_pow = 2.0;
const float k_speed_state_pow = 0.2;
const float k_loading_radius_mult = 0.7;
const float k_normal_radius_mult = 1.08;

const float k_bg_wave_freq = 24.0;
const float k_bg_diag_threshold = 0.4;

const float k_hex_ring_radius = 0.15;
const float k_hex_ring_width = 0.02;

const float k_core_circle_radius = 0.06;
const float k_core_circle_width = 0.01;

const float k_orbit_outer_base = 0.25;
const float k_orbit_outer_step = 0.12;
const float k_orbit_inner_base = 0.08;
const float k_orbit_inner_step = 0.05;
const float k_orbit_ring_width_base = 0.003;
const float k_orbit_ring_width_step = 0.001;
const float k_orbit_outer_spread = 1.12;
const float k_orbit_outer_spacing_pow = 1.4;
const float k_orbit_count = 4.0;
const float k_orbit_extra_count = 2.0;
const float k_orbit_extra_distance_mult = 1.4;
const float k_orbit_extra_inset = 0.25;
const float k_orbit_total_count = k_orbit_count + k_orbit_extra_count;

const float k_orbit_speed_mult_base = 0.85;
const float k_orbit_speed_mult_step = 0.12;

const float k_dash_threshold = 0.1;
const float k_head_threshold = 0.9;

out vec4 outColor;

float aastep(float threshold, float value) {
    float w = fwidth(value) * 0.5;
    return smoothstep(threshold - w, threshold + w, value);
}

void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution.xy;
    uv = uv * 2.0 - 1.0;
    uv.x *= u_resolution.x / u_resolution.y;

    // --- 1. Background ---
    vec3 bg_color = vec3(0.0);

    float bgWave = sin((uv.x + uv.y) * k_bg_wave_freq + u_time * 0.5);
    float bgDiag = aastep(k_bg_diag_threshold, bgWave);
    bg_color += vec3(0.02, 0.03, 0.08) * bgDiag;

    // --- Distance Metric ---
    // 0.866025 is approx sqrt(3)/2, creating a pointy-topped hexagon.
    vec2 p = abs(uv);
    float hex_dist = max(p.x, p.x * 0.5 + p.y * 0.866025);
    float circle_dist = length(uv);

    // --- 2. Glowing Center (Circular) ---
    float core_glow = exp(-circle_dist * 5.0);
    float core_ring = (1.0 - aastep(k_hex_ring_width, abs(hex_dist - k_hex_ring_radius))) * 0.5;
    float core_circle = 1.0 - aastep(k_core_circle_width, abs(circle_dist - k_core_circle_radius));
    vec3 center_color = vec3(0.06, 0.25, 0.4) * (core_glow + core_ring);
    center_color += vec3(0.08, 0.3, 0.5) * core_circle;

    // --- 3. Circling Lines (Orbitals) ---
    float angle = atan(uv.y, uv.x);
    vec3 orbital_color = vec3(0.0);
    float state_index = clamp(u_state, 0.0, k_state_max);
    float prev_state = clamp(u_prev_state, 0.0, k_state_max);
    float transition_seconds = max(k_state_transition_seconds, 0.0001);
    float state_elapsed = u_time - u_last_changed_time;
    float state_t = (u_last_changed_time == 0.0)
        ? 1.0 : clamp(state_elapsed / transition_seconds, 0.0, 1.0);
    float state_ease = state_t * state_t * (3.0 - 2.0 * state_t);
    float state_mix = mix(prev_state, state_index, state_ease);
    float distance_factor = state_mix / k_state_max;

    float prev_distance = prev_state / k_state_max;
    float curr_distance = state_index / k_state_max;
    float speed_prev = mix(k_loading_speed_mult, k_normal_speed_mult, pow(prev_distance, k_speed_state_pow));
    float speed_curr = mix(k_loading_speed_mult, k_normal_speed_mult, pow(curr_distance, k_speed_state_pow));

    float phase_time = u_time * speed_curr;
    if (u_last_changed_time != 0.0) {
        float t0 = u_last_changed_time;
        float s = state_t;
        float speed_s = s;
        float speed_e_int = s * s * s - 0.5 * s * s * s * s;

        if (speed_curr < speed_prev) {
            float p = k_speed_drop_pow;
            speed_s = 1.0 - pow(1.0 - s, p);
            speed_e_int = s - (1.0 - pow(1.0 - s, p + 1.0)) / (p + 1.0);
        }

        float transition_integral = transition_seconds *
                (speed_prev * s + (speed_curr - speed_prev) * speed_e_int);
        float after = max(u_time - t0 - transition_seconds, 0.0);
        phase_time = speed_prev * t0 + transition_integral + speed_curr * after;
    }

    float outer_spread_base = k_orbit_outer_base + k_orbit_count * k_orbit_outer_step * (1.0 - k_orbit_outer_spread);
    float outer_min = (outer_spread_base + 1.0 * k_orbit_outer_step * k_orbit_outer_spread)
            * k_normal_radius_mult;
    float outer_max = (outer_spread_base + k_orbit_count * k_orbit_outer_step * k_orbit_outer_spread)
            * k_normal_radius_mult;

    for (float i = 1.0; i <= k_orbit_total_count; i++) {
        float t = clamp((i - 1.0) / (k_orbit_count - 1.0), 0.0, 1.0);
        float outer_r = mix(outer_min, outer_max, pow(t, k_orbit_outer_spacing_pow));
        float inner_r = (k_orbit_inner_base + i * k_orbit_inner_step) * k_loading_radius_mult;
        float base_r = mix(inner_r, outer_r, distance_factor);

        float extra_mask = step(k_orbit_count + 0.5, i);
        float extra_index = i - k_orbit_count;
        float extra_step = (k_orbit_extra_distance_mult - 1.0) * outer_max;
        float extra_target = outer_max + extra_index * extra_step;
        float extra_r = extra_target + (1.0 - distance_factor) * k_orbit_extra_inset;
        float r = mix(base_r, extra_r, extra_mask);

        float base_speed = (mod(i, 2.0) == 0.0 ? 1.0 : -1.0) * (0.3 + i * 0.15);
        float speed_mult = k_orbit_speed_mult_base + k_orbit_speed_mult_step * i;
        float segments = 2.0 + i;
        float current_angle = angle * segments + phase_time * base_speed * speed_mult;

        float ring_width = k_orbit_ring_width_base + k_orbit_ring_width_step * i;

        float ring_mask = 1.0 - aastep(ring_width, abs(hex_dist - r));
        float dash = aastep(k_dash_threshold, sin(current_angle));
        float head = aastep(k_head_threshold, fract(current_angle / 6.28318));

        float ring_alpha = mix(1.0, distance_factor, extra_mask);
        vec3 col = 0.5 + 0.5 * cos(u_time * 0.4 + i + vec3(0.0, 1.5, 3.0));
        orbital_color += ring_mask * dash * col * (0.8 + head * 2.5) * ring_alpha;
    }

    vec3 final_color = vec3(0.03, 0.03, 0.05) + bg_color + center_color + orbital_color;

    outColor = vec4(final_color, 1.0);
}
