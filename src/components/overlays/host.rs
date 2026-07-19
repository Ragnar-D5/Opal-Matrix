use leptos::{portal::Portal, prelude::*};
use web_sys::KeyboardEvent;

use crate::components::overlays::{
    emoji_picker::{EmojiPickerPanel, EmojiPickerState},
    gif_picker::{GifPickerPanel, GifPickerState},
    profile_card::{ProfileCardPanel, ProfileCardState},
    space_search::{SpaceSearchPanel, SpaceSearchState},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverlayKind {
    EmojiPicker,
    GifPicker,
    ProfileCard,
    SpaceSearch,
}

#[component]
pub fn OverlayHost() -> impl IntoView {
    let emoji_state: EmojiPickerState = expect_context();
    let gif_state: GifPickerState = expect_context();
    let profile_state: ProfileCardState = expect_context();
    let space_state: SpaceSearchState = expect_context();

    let active = move || {
        if profile_state.is_open() {
            Some(OverlayKind::ProfileCard)
        } else if emoji_state.is_open() {
            Some(OverlayKind::EmojiPicker)
        } else if gif_state.is_open() {
            Some(OverlayKind::GifPicker)
        } else if space_state.is_open() {
            Some(OverlayKind::SpaceSearch)
        } else {
            None
        }
    };

    let close_active = move || match active() {
        Some(OverlayKind::EmojiPicker) => emoji_state.close(None),
        Some(OverlayKind::GifPicker) => gif_state.close(None),
        Some(OverlayKind::ProfileCard) => profile_state.close(),
        Some(OverlayKind::SpaceSearch) => space_state.close(),
        None => {}
    };

    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if active().is_some() && ev.key() == "Escape" {
            close_active();
        }
    });

    view! {
        <Show when=move || active().is_some()>
            <Portal>
                <div class="fixed inset-0 z-[999]" on:click=move |_| close_active() />
                {move || match active() {
                    Some(OverlayKind::EmojiPicker) => view! { <EmojiPickerPanel /> }.into_any(),
                    Some(OverlayKind::GifPicker) => view! { <GifPickerPanel /> }.into_any(),
                    Some(OverlayKind::ProfileCard) => view! { <ProfileCardPanel /> }.into_any(),
                    Some(OverlayKind::SpaceSearch) => view! { <SpaceSearchPanel /> }.into_any(),
                    None => ().into_any(),
                }}
            </Portal>
        </Show>
    }
}
