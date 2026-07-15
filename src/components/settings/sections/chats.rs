use leptos::prelude::*;

use crate::components::settings::{
    Settings,
    definition::system_message_modes,
    sections::{EnumToggle, Spacer, SubSection, Toggle},
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
            <Spacer />
            <EnumToggle
                field=settings.system_messages_to_show
                modes=system_message_modes().to_vec()
            />
        </SubSection>
    }
    .into_any()
}
