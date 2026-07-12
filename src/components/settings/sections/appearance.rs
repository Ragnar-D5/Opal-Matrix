use leptos::prelude::*;

use crate::components::settings::{sections::Slider, Settings};

pub fn render_appearance_section() -> AnyView {
    let settings: Settings = expect_context();

    view! { <Slider field=settings.scaling min=0.5 max=2.0 /> }.into_any()
}
