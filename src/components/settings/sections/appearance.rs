use leptos::prelude::*;

use crate::components::settings::{
    Settings,
    sections::{DiscreteSlider, Slider, Spacer},
};

pub fn render_appearance_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <DiscreteSlider
            field=settings.scaling
            values=(0..=6).map(|x| 0.5 + 0.25 * x as f64).collect()
        />
        <Spacer />
        <Slider field=settings.epstein_mode min=0.0 max=100.0 />
    }
    .into_any()
}
