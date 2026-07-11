use leptos::prelude::*;

use crate::components::settings::{
    sections::{Spacer, SubSection, Toggle},
    Settings,
};

pub fn render_chats_section() -> AnyView {
    let settings: Settings = expect_context();

    view! {
        <SubSection title="Indicators">
            <Toggle field=settings.show_read_markers />
            <Toggle field=settings.send_read_markers />
            <Spacer />
            <Toggle field=settings.show_typing_indicators />
            <Toggle field=settings.send_typing_indicators />
        </SubSection>
        <SubSection title="Messages">
            <Toggle field=settings.url_previews_default />
            <Toggle field=settings.mark_pinned_messages />
        </SubSection>
    }
    .into_any()
}
