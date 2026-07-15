use blurhash::decode;
use js_sys::Uint8ClampedArray;
use leptos::{html::Canvas, prelude::*};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, ImageData};

#[component]
pub fn Blurhash(hash: String) -> impl IntoView {
    let canvas_ref: NodeRef<Canvas> = NodeRef::new();

    Effect::new(move |_| {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };

        canvas.set_width(32);
        canvas.set_height(32);

        let Ok(pixels) = decode(&hash, 32, 32, 1.0) else {
            return;
        };

        let ctx = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok());

        let Some(ctx) = ctx else { return };

        let array = Uint8ClampedArray::from(pixels.as_slice());
        let Ok(image_data) = ImageData::new_with_js_u8_clamped_array_and_sh(&array, 32, 32) else {
            return;
        };

        let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
    });

    view! {
        <canvas node_ref=canvas_ref style="width: 100%; height: 100%; display: block;" />
    }
}
