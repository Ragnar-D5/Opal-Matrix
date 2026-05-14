use std::{cell::RefCell, rc::Rc};

use leptos::{html::Canvas, prelude::*};
use wasm_bindgen::{prelude::Closure, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Response, WebGl2RenderingContext};

#[component]
pub fn BackgroundShader() -> impl IntoView {
    let canvas_ref: NodeRef<Canvas> = NodeRef::new();

    // Load the shader file from the assets folder
    let shader_source = LocalResource::new(|| async move {
        let window = web_sys::window()?;
        let resp_value = JsFuture::from(window.fetch_with_str("/public/background_center.glsl"))
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

            let vert_src = r#"#version 300 es
                        in vec2 position;
                        void main() {
                            gl_Position = vec4(position, 0.0, 1.0);
                        }"#;

            let program = setup_webgl_program(&gl, vert_src, &frag_src);

            // --- ANIMATION LOOP SETUP ---
            let gl_rc = Rc::new(gl);
            let program_rc = Rc::new(program);
            let canvas_rc = Rc::new(canvas);
            let start_time = web_sys::window().unwrap().performance().unwrap().now();

            let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
            let g = f.clone();

            *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
                let current_time = web_sys::window().unwrap().performance().unwrap().now();
                let time_sec = ((current_time - start_time) / 1000.0) as f32;

                gl_rc.use_program(Some(&program_rc));

                // 1. Handle Resize & Resolution
                let width = canvas_rc.client_width() as f32;
                let height = canvas_rc.client_height() as f32;

                if canvas_rc.width() as f32 != width || canvas_rc.height() as f32 != height {
                    canvas_rc.set_width(width as u32);
                    canvas_rc.set_height(height as u32);
                    gl_rc.viewport(0, 0, width as i32, height as i32);
                }

                // 2. Set Uniforms
                let res_loc = gl_rc.get_uniform_location(&program_rc, "u_resolution");
                gl_rc.uniform2f(res_loc.as_ref(), width, height);

                let time_loc = gl_rc.get_uniform_location(&program_rc, "u_time");
                gl_rc.uniform1f(time_loc.as_ref(), time_sec);

                // 3. Draw
                gl_rc.draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, 6);

                // Schedule next frame
                if let Some(window) = web_sys::window() {
                    let _ = window.request_animation_frame(
                        f.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
                    );
                }
            }) as Box<dyn FnMut()>));

            // Start the first frame
            let _ = web_sys::window()
                .unwrap()
                .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref());
        }
    });

    view! {
        <canvas
            node_ref=canvas_ref
            class="fixed top-0 left-0 w-screen h-screen -z-10 pointer-events-none"
        />
    }
}

fn setup_webgl_program(
    gl: &WebGl2RenderingContext,
    vert_src: &str,
    frag_src: &str,
) -> web_sys::WebGlProgram {
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

    program
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
