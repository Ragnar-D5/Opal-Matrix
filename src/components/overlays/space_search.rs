use leptos::{
    html::{Div, Input},
    prelude::*,
    task::spawn_local,
};
use leptos_use::{UseIntersectionObserverReturn, use_intersection_observer};
use phosphor_leptos::{Icon, IconWeight, MAGNIFYING_GLASS};
use ruma::directory::{PublicRoomsChunk, RoomTypeFilter};
use uuid::Uuid;
use web_sys::IntersectionObserverEntry;

use crate::{
    hooks::use_tauri_channel,
    tauri_functions::{
        close_room_search, load_more_room_search, open_room_search, search_room_directory,
    },
};

/// Wraps the channel payload so it can satisfy `use_tauri_channel`'s
/// `PartialEq` bound — `PublicRoomsChunk` doesn't derive it upstream, so
/// equality here is a cheap approximation based on the room id sequence.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct RoomSearchResults(pub Vec<PublicRoomsChunk>);

impl PartialEq for RoomSearchResults {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|(a, b)| a.room_id == b.room_id)
    }
}

#[derive(Clone, Copy)]
pub struct SpaceSearchState {
    open: RwSignal<bool>,
    anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>>,
}

impl Default for SpaceSearchState {
    fn default() -> Self {
        Self {
            open: RwSignal::new(false),
            anchor_rect: RwSignal::new(None),
        }
    }
}

impl SpaceSearchState {
    fn close(&self) {
        self.open.set(false);
    }
}

pub fn open_space_search(anchor: &web_sys::Element, state: SpaceSearchState) {
    let rect = anchor.get_bounding_client_rect();
    state
        .anchor_rect
        .set(Some((rect.left(), rect.top(), rect.right(), rect.bottom())));
    state.open.set(true);
}

#[component]
pub fn SpaceSearchPortal() -> impl IntoView {
    let state: SpaceSearchState = expect_context();

    let session_id: RwSignal<Option<Uuid>> = RwSignal::new(None);
    let results: RwSignal<RoomSearchResults> = RwSignal::new(RoomSearchResults(Vec::new()));
    let has_more = RwSignal::new(true);
    let loading = RwSignal::new(false);
    let search_ref: NodeRef<Input> = NodeRef::new();
    let sentinel_ref: NodeRef<Div> = NodeRef::new();

    let channel = StoredValue::new_local(use_tauri_channel(results));

    let UseIntersectionObserverReturn { .. } = use_intersection_observer(
        sentinel_ref,
        move |entries: Vec<IntersectionObserverEntry>, _| {
            let Some(id) = session_id.get_untracked() else {
                return;
            };
            if entries[0].is_intersecting() && has_more.get_untracked() && !loading.get_untracked()
            {
                loading.set(true);
                spawn_local(async move {
                    match load_more_room_search(id).await {
                        Ok(at_end) => has_more.set(!at_end),
                        Err(e) => log::error!("Failed to load more spaces: {e}"),
                    }
                    loading.set(false);
                });
            }
        },
    );

    Effect::new(move |_| {
        if !state.open.get() {
            if let Some(id) = session_id.get_untracked() {
                session_id.set(None);
                spawn_local(async move {
                    if let Err(e) = close_room_search(id).await {
                        log::error!("Failed to close space search session: {e}");
                    }
                });
            }
            return;
        }

        let id = Uuid::new_v4();
        session_id.set(Some(id));
        results.set(RoomSearchResults(Vec::new()));
        has_more.set(true);

        let channel = channel.get_value();
        spawn_local(async move {
            if let Err(e) = open_room_search(id, vec![RoomTypeFilter::Space], &channel).await {
                log::error!("Failed to open space search: {e}");
                return;
            }
            match search_room_directory(id, None).await {
                Ok(at_end) => has_more.set(!at_end),
                Err(e) => log::error!("Failed to search spaces: {e}"),
            }
        });

        if let Some(el) = search_ref.get() {
            let _ = el.focus();
        }
    });

    let on_input = move |ev: leptos::ev::Event| {
        let Some(id) = session_id.get_untracked() else {
            return;
        };
        let value = event_target_value(&ev);
        let term = (!value.is_empty()).then_some(value);
        has_more.set(true);
        spawn_local(async move {
            match search_room_directory(id, term).await {
                Ok(at_end) => has_more.set(!at_end),
                Err(e) => log::error!("Failed to search spaces: {e}"),
            }
        });
    };

    window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if state.open.try_get_untracked().unwrap_or(false) && ev.key() == "Escape" {
            state.close();
        }
    });

    let style = move || {
        let Some((left, top, right, bottom)) = state.anchor_rect.get() else {
            return String::new();
        };

        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let vh = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);

        let picker_w: f64 = 320.0;
        let picker_h: f64 = 420.0;
        let offset = 12.0;

        let actual_h = picker_h.min(vh - offset * 2.0);

        let space_below = vh - bottom;
        let space_above = top;
        let place_below = space_below >= actual_h + offset || space_below > space_above;

        let y_style = if place_below {
            format!("top:{}px;", bottom + offset)
        } else {
            format!("bottom:{}px;", vh - top + offset)
        };

        let x_style = if right + offset + picker_w <= vw - offset {
            format!("left:{}px;", right + offset)
        } else {
            format!("left:{}px;", left)
        };

        format!("{x_style}{y_style}width:{picker_w}px;height:{actual_h}px;")
    };

    view! {
        <Show when=move || state.open.get()>
            <div class="fixed inset-0 z-[999]" on:click=move |_| state.close() />

            <div
                class="fixed z-[1000] flex flex-col bg-(--ui-floating-hover-bg) backdrop-blur-2xl border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-xl overflow-hidden"
                style=style
            >
                <div class="p-2 border-b border-(--tile-border-color) flex-shrink-0">
                    <div class="relative flex items-center">
                        <div class="absolute left-2 flex items-center pointer-events-none text-muted">
                            <Icon icon=MAGNIFYING_GLASS weight=IconWeight::Bold size="14px" />
                        </div>
                        <input
                            type="text"
                            node_ref=search_ref
                            placeholder="Find a space"
                            class="w-full ui-solid-bg border border-(--tile-border-color) rounded-ui pl-7 pr-2 py-1 text-sm text-normal outline-none placeholder:text-muted focus:border-(--focus-color)"
                            on:input=on_input
                        />
                    </div>
                </div>

                <div
                    class="overflow-y-auto flex-1 p-1.5 flex flex-col gap-1"
                    style="scrollbar-width: thin;"
                >
                    <Show when=move || results.get().0.is_empty() && !loading.get()>
                        <div class="w-full h-full flex flex-col items-center justify-center text-muted gap-2 pt-12">
                            <span class="text-sm">"No spaces found"</span>
                        </div>
                    </Show>

                    <For
                        each=move || results.get().0
                        key=|room| room.room_id.clone()
                        children=move |room| {
                            let name = room
                                .name
                                .clone()
                                .or_else(|| room.canonical_alias.as_ref().map(|a| a.to_string()))
                                .unwrap_or_else(|| room.room_id.to_string());
                            let topic = room.topic.clone();
                            let members = room.num_joined_members;

                            view! {
                                <div class="flex items-center gap-2 p-1.5 rounded-ui hover:bg-(--ui-solid-hover-bg)">
                                    <div class="flex flex-col min-w-0 flex-1">
                                        <span class="text-sm text-normal truncate">{name}</span>
                                        {topic
                                            .map(|t| {
                                                view! { <span class="text-xs text-dim truncate">{t}</span> }
                                            })}
                                    </div>
                                    <span class="text-xs text-muted flex-shrink-0">
                                        {members.to_string()}
                                    </span>
                                </div>
                            }
                        }
                    />

                    <div node_ref=sentinel_ref class="h-1 w-full" />
                </div>
            </div>
        </Show>
    }
}
