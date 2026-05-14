#version 300 es
precision highp float;

uniform float u_time;
uniform vec2 u_resolution;

out vec4 outColor;

void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution.xy;
    uv = uv * 2.0 - 1.0;
    uv.x *= u_resolution.x / u_resolution.y;

    // --- 1. Background Lines ---
    float bgX = smoothstep(0.98, 1.0, fract(uv.x * 6.0 + u_time * 0.1));
    float bgY = smoothstep(0.98, 1.0, fract(uv.y * 6.0 - u_time * 0.15));
    vec3 bg_color = vec3(0.06, 0.08, 0.15) * (bgX + bgY) * 0.4;

    float bgDiag = smoothstep(0.8, 1.0, sin((uv.x + uv.y) * 30.0 + u_time * 0.5));
    bg_color += vec3(0.02, 0.03, 0.08) * bgDiag;

    // --- Distance Metric ---
    // 0.866025 is approx sqrt(3)/2, creating a pointy-topped hexagon.
    vec2 p = abs(uv);
    float hex_dist = max(p.x, p.x * 0.5 + p.y * 0.866025);

    // --- 2. Glowing Center Hexagon ---
    // Increased the multiplier from 6.0 to 8.5 to pull the glow inwards (darker overall)
    float core_glow = exp(-hex_dist * 3.0);
    float core_ring = smoothstep(0.02, 0.0, abs(hex_dist - 0.15)) * 0.5;

    // Lowered the base color intensity from (0.2, 0.6, 1.0) to make it darker
    vec3 center_color = vec3(0.04, 0.17, 0.3) * (core_glow + core_ring);

    // --- 3. Circling Lines (Orbitals) ---
    float angle = atan(uv.y, uv.x);
    vec3 orbital_color = vec3(0.0);

    for (float i = 1.0; i <= 4.0; i++) {
        float r = 0.25 + i * 0.12;

        float speed = (mod(i, 2.0) == 0.0 ? 1.0 : -1.0) * (0.3 + i * 0.15);
        float segments = 2.0 + i;
        float current_angle = angle * segments + u_time * speed;

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
