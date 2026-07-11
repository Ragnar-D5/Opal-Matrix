use leptos::prelude::*;

use crate::components::settings::{
    sections::{Dropdown, Spacer, SubSection},
    Settings,
};

pub fn render_general_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <SubSection title="Language/Region">
            <Dropdown field=settings.hour_format />
            <Dropdown field=settings.date_format />
            <Dropdown field=settings.first_day_of_week />
            <Spacer />
            <Dropdown field=settings.timezone />
        </SubSection>
        <SubSection title="Units">
            <Dropdown field=settings.data_size_unit />
        </SubSection>
    }
    .into_any()
}
