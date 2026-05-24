const VERT_SRC = `#version 300 es
in vec2 position;
void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}`;

let gl = null;
let prog = null;
let startSec = null;

// Cached uniform locations
let locRes, locT, locLC, locSt, locPSt;

// State pushed from main thread (Leptos)
let uLC = 0.0;
let uSt = 0.0;
let uPSt = 0.0;

export function init_shader(canvas) {
  startSec = performance.now() / 1000.0;

  gl = canvas.getContext("webgl2");
  if (!gl) {
    console.error("[Shader] WebGL2 not available");
    return;
  }

  // Keep the WebGL canvas buffer size in sync with its physical CSS size
  const resize = () => {
    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;
    gl.viewport(0, 0, canvas.width, canvas.height);
  };
  window.addEventListener("resize", resize);
  resize(); // Trigger immediately to set initial dimensions

  // Fetch the GLSL source then start the loop.
  fetch("/public/background_center.glsl")
    .then((r) => {
      if (!r.ok) throw new Error("Failed to fetch shader: " + r.status);
      return r.text();
    })
    .then((fragSrc) => {
      setup(fragSrc);
      requestAnimationFrame(render);
    })
    .catch((err) => console.error("[Shader]", err));
}

export function update_shader_state(current, prev, lastChangedTime) {
  uSt = current;
  uPSt = prev;
  uLC = lastChangedTime;
}

// ---------------------------------------------------------------------------

function compileShader(type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS))
    console.error("[Shader] compile error:", gl.getShaderInfoLog(s));
  return s;
}

function setup(fragSrc) {
  prog = gl.createProgram();
  gl.attachShader(prog, compileShader(gl.VERTEX_SHADER, VERT_SRC));
  gl.attachShader(prog, compileShader(gl.FRAGMENT_SHADER, fragSrc));
  gl.linkProgram(prog);

  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    console.error("[Shader] link error:", gl.getProgramInfoLog(prog));
    return;
  }

  gl.useProgram(prog);

  // Full-screen quad — two triangles
  const verts = new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, verts, gl.STATIC_DRAW);

  const posLoc = gl.getAttribLocation(prog, "position");
  gl.enableVertexAttribArray(posLoc);
  gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0);

  // Cache all uniform locations — avoids a lookup per frame
  locRes = gl.getUniformLocation(prog, "u_resolution");
  locT = gl.getUniformLocation(prog, "u_time");
  locLC = gl.getUniformLocation(prog, "u_last_changed_time");
  locSt = gl.getUniformLocation(prog, "u_state");
  locPSt = gl.getUniformLocation(prog, "u_prev_state");
}

function render() {
  if (!gl || !prog) return;

  const t = performance.now() / 1000.0 - startSec;
  const lc = uLC === 0.0 ? 0.0 : uLC - startSec;

  gl.uniform2f(locRes, gl.canvas.width, gl.canvas.height);
  gl.uniform1f(locT, t);
  gl.uniform1f(locLC, lc);
  gl.uniform1f(locSt, uSt);
  gl.uniform1f(locPSt, uPSt);

  gl.drawArrays(gl.TRIANGLES, 0, 6);
  requestAnimationFrame(render);
}
