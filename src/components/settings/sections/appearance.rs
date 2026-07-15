use leptos::prelude::*;

use crate::components::settings::{
    Settings,
    sections::{Slider, Spacer},
};

pub fn render_appearance_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <Slider field=settings.scaling min=25.0 max=300.0 />
        <Spacer />
        <Slider field=settings.epstein_mode min=0.0 max=100.0 />
    }
    .into_any()
}
