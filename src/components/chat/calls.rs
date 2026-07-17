use leptos::prelude::*;
use leptos_use::use_resize_observer;
use shared::{ColorExt, sidebar::RoomNode};
use web_sys::ResizeObserverEntry;

use crate::{
    components::user_profile::MemberProfileExt,
    state::{AppState, MediaCache, ProfileStore},
};

#[component]
pub fn CallView(node: RoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let container_ref = NodeRef::<leptos::html::Div>::new();
    let (is_large, set_is_large) = signal(false);
    use_resize_observer(
        container_ref,
        move |entries: Vec<ResizeObserverEntry>, _| {
            if let Some(entry) = entries.first() {
                let rect = entry.content_rect();
                set_is_large.set(rect.width() > 400.0 && rect.height() > 400.0);
            }
        },
    );

    let room_id = node.room_id();
    let part_room_id = room_id.clone();
    let participants = Memo::new(move |_| state.get_call_members(&part_room_id).get());

    let name = node.name();

    let content_name = name.clone();
    let content = move || {
        let participants = participants.get();
        if participants.is_empty() {
            view! {
                <div class="flex-1 flex items-center justify-center text-muted flex-col gap-2 bg-radial-[at_50%_100%] from-(--accent-color) to-transparent to-80% w-full h-full">
                    <span class="text-3xl text-bright font-bold text-shadow-xs">
                        {content_name.clone()}
                    </span>
                    <span class="text-muted">"No one is currently in this voice channel"</span>
                </div>
            }
            .into_any()
        } else {
            let count = participants.len();
            let width_class = match count {
                1 => "w-full max-w-5xl",
                2 => "w-[calc(50%-0.5*var(--gap))] max-w-3xl",
                3 | 4 => "w-[calc(50%-0.5*var(--gap))] max-w-2xl",
                5..=6 => "w-[calc(33.33%-0.66*var(--gap))] max-w-xl",
                7..=9 => "w-[calc(33.33%-0.66*var(--gap))] max-w-lg",
                10..=12 => "w-[calc(25%-0.75*var(--gap))] max-w-md",
                _ => "w-[calc(20%-0.8*var(--gap))] max-w-sm",
            };

            view! {
                <div class="flex-1 flex flex-wrap justify-center content-center w-full h-full min-h-0 gap-[var(--gap)] p-[var(--gap)] overflow-y-auto">
                    {participants
                        .iter()
                        .map(|device| {
                            let profile = store
                                .get_member_profile(&node.room_id(), &device.user_id);
                            let clone = profile.clone();
                            let colors = move || {
                                let mut color = clone.get().banner_color();
                                let fg_color = color.clone().to_css_hsl();
                                color.set_lightness(0.1);
                                format!(
                                    "background-color: {}; box-shadow: inset 0 0 20px 0px {};",
                                    color.to_css_hsl(),
                                    fg_color,
                                )
                            };
                            let clone = profile.clone();
                            view! {
                                <div
                                    class=move || {
                                        if is_large.get() {
                                            format!(
                                                "{} aspect-video rounded-3xl flex flex-col items-center justify-center overflow-hidden transition-all duration-300",
                                                width_class,
                                            )
                                        } else {
                                            "flex flex-col items-center justify-center transition-all duration-300"
                                                .to_string()
                                        }
                                    }
                                    style=move || {
                                        if is_large.get() { colors() } else { String::new() }
                                    }
                                >
                                    {move || profile.get().render_icon("64px", cache)}
                                    {move || {
                                        is_large
                                            .get()
                                            .then(|| clone.get().render_name_popup("16px"))
                                    }}
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            }
            .into_any()
        }
    };

    view! {
        <div node_ref=container_ref class="flex w-full h-full ui-solid-bg">
            {content}
        </div>
    }
    .into_any()
}
