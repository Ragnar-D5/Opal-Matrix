use crate::{app::CurrentWindow, state::AppState};
use leptos::{html::Canvas, prelude::*};
use wasm_bindgen::prelude::*;

// Bind to your newly created module
#[wasm_bindgen(module = "/src/shader.js")]
extern "C" {
    fn init_shader(canvas: &web_sys::HtmlCanvasElement);
    fn update_shader_state(current: f64, prev: f64, last_changed: f64);
}

fn window_to_f64(w: CurrentWindow) -> f64 {
    match w {
        CurrentWindow::Loading => 0.0,
        CurrentWindow::HomeserverDiscovery => 1.0,
        CurrentWindow::Login => 2.0,
        CurrentWindow::Home => 3.0,
    }
}

#[component]
pub fn BackgroundShader() -> impl IntoView {
    let canvas_ref: NodeRef<Canvas> = NodeRef::new();
    let state: AppState = expect_context();

    // Init effect
    Effect::new(move |_| {
        if let Some(canvas) = canvas_ref.get() {
            // Leptos Canvas derefs to web_sys::HtmlCanvasElement
            init_shader(&canvas);
        }
    });

    // State-change effect
    Effect::new(move |_| {
        let current = window_to_f64(state.current_window.get());
        let prev = window_to_f64(state.previous_window.get());
        let last_changed = state.last_changed_time.get();

        update_shader_state(current, prev, last_changed);
    });

    view! {
        <canvas
            node_ref=canvas_ref
            class="fixed top-0 left-0 w-screen h-screen -z-10 pointer-events-none"
        />
    }
}
