use leptos::{html::Canvas, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Response, WebGl2RenderingContext};

#[component]
pub fn BackgroundShader() -> impl IntoView {
    let canvas_ref: NodeRef<Canvas> = NodeRef::new();

    // Load the shader file from the assets folder
    let shader_source = LocalResource::new(|| async move {
        let window = web_sys::window()?;
        let resp_value = JsFuture::from(window.fetch_with_str("/public/background.glsl"))
            .await
            .ok()?;
        let resp: Response = resp_value.dyn_into().ok()?;

        if !resp.ok() {
            return None;
        }

        let text_value = JsFuture::from(resp.text().ok()?).await.ok()?;
        text_value.as_string()
    });

    Effect::new(move |_| {
        if let (Some(canvas), Some(Some(frag_src))) = (canvas_ref.get(), shader_source.get()) {
            let gl = canvas
                .get_context("webgl2")
                .unwrap()
                .unwrap()
                .dyn_into::<WebGl2RenderingContext>()
                .unwrap();

            // 1. SYNC PIXELS TO CSS SIZE
            let width = canvas.client_width() as u32;
            let height = canvas.client_height() as u32;
            canvas.set_width(width);
            canvas.set_height(height);
            gl.viewport(0, 0, width as i32, height as i32);

            let vert_src = r#"#version 300 es
                in vec2 position;
                void main() {
                    gl_Position = vec4(position, 0.0, 1.0);
                }"#;

            setup_webgl_program(&gl, vert_src, &frag_src);
        }
    });

    view! {
        <canvas
            node_ref=canvas_ref
            class="fixed top-0 left-0 w-screen h-screen -z-10 pointer-events-none"
        />
    }
}

fn setup_webgl_program(gl: &WebGl2RenderingContext, vert_src: &str, frag_src: &str) {
    // Helper to compile and link shaders
    let vert_shader = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, vert_src);
    let frag_shader = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, frag_src);

    let program = gl.create_program().unwrap();
    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);
    gl.use_program(Some(&program));

    // Create a full-screen quad (two triangles)
    let vertices: [f32; 12] = [
        -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0,
    ];

    let buffer = gl.create_buffer().unwrap();
    gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));
    unsafe {
        let view = js_sys::Float32Array::view(&vertices);
        gl.buffer_data_with_array_buffer_view(
            WebGl2RenderingContext::ARRAY_BUFFER,
            &view,
            WebGl2RenderingContext::STATIC_DRAW,
        );
    }

    let pos_attr = gl.get_attrib_location(&program, "position") as u32;
    gl.enable_vertex_attrib_array(pos_attr);
    gl.vertex_attrib_pointer_with_i32(pos_attr, 2, WebGl2RenderingContext::FLOAT, false, 0, 0);

    gl.draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, 6);
}

fn compile_shader(
    gl: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> web_sys::WebGlShader {
    let shader = gl.create_shader(shader_type).unwrap();
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    shader
}
