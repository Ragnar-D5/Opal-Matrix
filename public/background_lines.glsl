#version 300 es
precision highp float;

uniform float u_time;
uniform vec2 u_resolution;

out vec4 outColor;

// Simple hash function for randomness
float hash(float n) {
    return fract(sin(n) * 43758.5453123);
}

void main() {
    float aspect = u_resolution.x / u_resolution.y;
    vec2 st = gl_FragCoord.xy / u_resolution.xy;
    st.x *= aspect;

    vec3 color = vec3(0.08, 0.08, 0.12);

    float lanes = 40.0;
    float lane_height = 1.0 / lanes;

    float num_kinks = 6.0;
    float spacing = (aspect + 1.0) / num_kinks;
    float total_lines = 18.0;

    for (float i = 0.0; i < total_lines; i++) {
        // --- 1. Routing (Unchanged) ---
        float start_lane = floor(hash(i * 123.456) * (lanes + 10.0)) - 5.0;
        float h = start_lane * lane_height;

        float y_offset = 0.0;
        float on_slope = 0.0;

        for (float k = 0.0; k < 6.0; k++) {
            float dir = sign(hash(i * 11.1 + k * 22.2) - 0.5);
            float shift_lanes = floor(hash(i * 33.3 + k * 44.4) * 3.0 + 1.0);
            float H = shift_lanes * lane_height;

            float random_x_offset = hash(i * 55.5 + k * 66.6) * (spacing - H - 0.02);
            float kink_x = -0.2 + k * spacing + random_x_offset;

            float dx = st.x - kink_x;
            y_offset += clamp(dx, 0.0, H) * dir;

            on_slope += step(0.0, dx) * step(dx, H);
        }

        float path_y = h + y_offset;
        float dist_y = abs(st.y - path_y) / (1.0 + min(on_slope, 1.0) * 0.4142);
        float line_mask = smoothstep(0.003, 0.0, dist_y);

        // --- 2. Spacing and Movement ---
        // Force the speed to be nearly identical so they don't catch up to each other
        float speed = 0.01 + hash(i * 99.9) * 0.002;
        // Perfectly distribute their starting phases from 0.0 to 1.0
        float phase = i / total_lines;

        float raw_x = fract(u_time * speed + phase);
        // Expand the X bounds slightly so the long tail fully enters and exits the screen
        float x_pos = (raw_x * (aspect + 2.0)) - 1.0;

        // --- 3. Comet Shape ---
        float dist_x = st.x - x_pos;

        // step() acts as a strict cutoff: 1.0 behind the comet, 0.0 in front of it
        float behind_comet = step(dist_x, 0.0);

        // Fading tail stretching 0.7 units backwards
        float tail = smoothstep(-0.7, 0.0, dist_x) * behind_comet;
        // Tiny, ultra-bright core at the very front
        float head = smoothstep(-0.015, 0.0, dist_x) * behind_comet;

        float length_mask = tail + (head * 3.0);

        // --- 4. Color ---
        vec3 line_color = 0.5 + 0.5 * cos(u_time * 0.6 + (i * 2.34) + vec3(0.0, 2.0, 4.0));
        color += line_color * line_mask * length_mask * 0.8;
    }

    outColor = vec4(color, 1.0);
}
