use leptos::prelude::*;

use crate::{
    components::settings::{
        sections::{Slider, Spacer},
        Settings,
    },
    tauri_functions::change_screen_scaling,
};

pub fn render_appearance_section() -> AnyView {
    let settings: Settings = expect_context();

    let scaling_sig = settings.scaling.signal();

    Effect::new(move |_| {
        change_screen_scaling(scaling_sig.get());
    });

    view! {
        <Slider field=settings.scaling min=0.5 max=2.0 />
        <Spacer />
        <Slider field=settings.epstein_mode min=0.0 max=1.0 />
    }
    .into_any()
}
