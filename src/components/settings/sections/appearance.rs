use leptos::prelude::*;

use crate::tauri_functions::change_screen_scaling;

pub fn render_appearance_section() -> AnyView {
    let (slider_value, set_slider_value) = signal(50);

    view! {
        <div class="flex items-center justify-between p-4 bg-neutral-800/40 rounded-xl border border-neutral-800/80 w-full">
            <div class="flex flex-col space-y-1 pr-4">
                <label for="scaling-slider" class="text-sm font-semibold text-bright">
                    "UI scale"
                </label>
                <span class="text-xs text-muted font-mono">
                    {move || { format!("{:.2}x", 0.5 + (slider_value.get() as f64 / 100.0) * 1.5) }}
                </span>
            </div>

            <div class="w-full max-w-xs md:max-w-md">
                <input
                    id="scaling-slider"
                    type="range"
                    min="0"
                    max="100"
                    prop:value=move || slider_value.get()

                    on:input=move |ev| {
                        let val = event_target_value(&ev).parse::<i32>().unwrap_or(0);
                        set_slider_value.set(val);
                        let mapped_val = 0.5 + (val as f64 / 100.0) * 1.5;
                        change_screen_scaling(mapped_val);
                    }
                    class="w-full h-2 bg-neutral-700 rounded-lg appearance-none cursor-pointer accent-indigo-500"
                />
            </div>
        </div>
    }.into_any()
}
