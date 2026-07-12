use leptos::prelude::*;

use crate::components::settings::{
    sections::{Slider, Spacer},
    Settings,
};

pub fn render_appearance_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <Slider field=settings.scaling min=0.0 max=2.0 />
        <Spacer />
        <Slider field=settings.epstein_mode min=0.0 max=1.0 />
    }
    .into_any()
}
