use leptos::prelude::*;

use crate::components::settings::{
    Settings,
    sections::{Dropdown, Spacer, SubSection, Toggle},
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
        <SubSection title="Behavior">
            <Toggle field=settings.minimize_to_tray />
        </SubSection>
    }
    .into_any()
}
