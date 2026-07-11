use leptos::prelude::*;

use crate::{
    app::Settings,
    components::settings::sections::{SubSection, Toggle},
};

pub fn render_chats_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <SubSection title="General">
            <Toggle field=settings.show_read_markers />
            <Toggle field=settings.show_typing_indicators />
            <Toggle field=settings.send_typing_indicators />
        </SubSection>
        <SubSection title="Url Previews">
            <Toggle field=settings.url_previews_default />
        </SubSection>
    }
    .into_any()
}
