#version 300 es
precision highp float;

out vec4 outColor;

void main() {
    // Coordinate mapping: center the set and scale it
    vec2 uv = (gl_FragCoord.xy - vec2(800.0, 450.0)) / 400.0;
    vec2 c = uv - vec2(0.5, 0.0);
    vec2 z = vec2(0.0);

    int iter = 0;
    int maxIter = 100;

    for (int i = 0; i < maxIter; i++) {
        // Standard Mandelbrot: z = z^2 + c
        z = vec2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
        if (length(z) > 2.0) break;
        iter++;
    }

    float color = float(iter) / float(maxIter);
    outColor = vec4(vec3(color), 1.0);
}
