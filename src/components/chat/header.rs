use std::collections::HashMap;

use leptos::{html::Input, prelude::*, task::spawn_local};
use leptos_use::use_active_element;
use phosphor_leptos::{
    INFO, Icon, IconWeight, MAGNIFYING_GLASS, PHONE, PHONE_DISCONNECT, PUSH_PIN, USER_CIRCLE,
    USER_LIST, X,
};
use ruma::OwnedRoomId;
use shared::{api::SearchParameters, sidebar::RoomNode, timeline::UiTimelineItem};
use uuid::Uuid;
use web_sys::KeyboardEvent;

use crate::{
    components::{
        FloatingTile, SystemButtons,
        user_profile::{MemberProfileExt, RoomNodeExt},
    },
    state::{AppState, CurrentSection, MediaCache, ProfileStore},
    tauri_functions::{get_pinned_events, join_call, leave_call, search_rooms},
};

#[component]
pub fn ChatHeader(chat_sidebar_open: RwSignal<bool>) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let toggle_icon = move || {
        if let Some(node) = state.active_room.get() {
            if node.is_dm() {
                USER_CIRCLE
            } else if node.has_children() {
                INFO
            } else {
                USER_LIST
            }
        } else {
            INFO
        }
    };

    let name = move || {
        if let CurrentSection::Server(server_id) = state.active_section.get() {
            let server = state.get_room(&server_id)?;

            Some(server.name())
        } else {
            let node = state.active_room.get()?;

            Some(node.name())
        }
    };

    let search_params: RwSignal<Option<SearchParameters>> = expect_context();
    let search_results: RwSignal<Option<HashMap<OwnedRoomId, Vec<UiTimelineItem>>>> =
        expect_context();

    let on_search_input = move |ev| {
        let input = event_target_value(&ev);

        let mut params = search_params.get().unwrap_or_default();
        params.text = input.clone();

        let active_room_id = state.active_room_id();

        if params.room_ids.is_empty()
            && let Some(room_id) = active_room_id.clone()
        {
            params.room_ids.push(room_id);
        }

        if params.is_empty(active_room_id.clone()) {
            search_params.set(None);
            search_results.set(None);

            return;
        }

        if input.len() < 3 {
            search_results.set(None);
            return;
        }

        let search_id = Uuid::new_v4();
        params.search_id = search_id;

        search_results.update(|results| {
            results
                .get_or_insert_with(HashMap::new)
                .retain(|room_id, _| params.room_ids.contains(room_id));
        });
        search_params.set(Some(params.clone()));

        search_rooms(params, search_id);
    };

    let search_input_ref: NodeRef<Input> = NodeRef::new();

    let on_search_keydown = move |ev: KeyboardEvent| {
        let Some(input) = search_input_ref.get() else {
            log::warn!("Search input not found");
            return;
        };

        match ev.key().as_str() {
            "Escape" => {
                search_params.set(None);
                search_results.set(None);

                input.set_value("");
                let _ = input.blur();
            }
            "Enter" => {
                let _ = input.blur();
            }
            _ => {}
        }
    };

    let input_empty = Memo::new(move |_| {
        search_params
            .get()
            .map(|p| p.text.is_empty())
            .unwrap_or(true)
    });

    let input_icon = move || {
        let icon = if input_empty.get() {
            MAGNIFYING_GLASS
        } else {
            X
        };

        view! { <Icon icon=icon size="16px" /> }
    };

    let active_element = use_active_element();
    let search_input_focused = move || {
        let current_active = active_element.get();
        let input_el = search_input_ref.get();

        match (current_active, input_el) {
            (Some(active), Some(input)) => active == input.into(),
            _ => false,
        }
    };

    let in_call = move || {
        let Some(room_id) = state.active_room_id() else {
            return false;
        };

        let user_id = state.device.get();
        state
            .get_call_members(&room_id)
            .get()
            .iter()
            .any(|dev| Some(dev) == user_id.as_ref())
    };

    let on_call_click = move |_| {
        let Some(room_id) = state.active_room_id() else {
            return;
        };

        if in_call() {
            leave_call(&room_id);
        } else {
            join_call(&room_id);
        }
    };

    let pinned_messages: RwSignal<Option<Vec<UiTimelineItem>>> = expect_context();

    Effect::new(move |_| {
        let Some(pinned) = pinned_messages.get() else {
            return;
        };

        if pinned.is_empty() {
            let Some(room_id) = state.active_room_id() else {
                return;
            };

            spawn_local(async move {
                match get_pinned_events(&room_id).await {
                    Ok(pinned) => {
                        if !pinned.is_empty() && pinned_messages.get_untracked() == Some(Vec::new())
                        {
                            pinned_messages.set(Some(pinned));
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch pinned messages: {e}");
                    }
                }
            });
        }
    });

    let on_pin_click = move |_| {
        if pinned_messages.get().is_some() {
            pinned_messages.set(None);
        } else {
            pinned_messages.set(Some(Vec::new()));
        }
    };

    view! {
        <FloatingTile class="h-(--header-height) items-start flex-row gap-1 pl-[5px]">
            <div class="w-8 self-center flex items-center justify-center text-normal">
                {move || {
                    let Some(node) = state.active_room.get() else {
                        return view! {
                            <div class="w-full justify-center flex">
                                <Icon icon=INFO color="currentColor" size="70%" />
                            </div>
                        }
                            .into_any();
                    };
                    view! { {node.render_icon("70%", cache)} }
                }}
            </div>
            <div class="flex-1 flex flex-col self-center text-normal text-m font-semibold">
                {move || {
                    let Some(current_room) = state.active_room.get() else {
                        return view! { <span>"No Room"</span> }.into_any();
                    };
                    if let RoomNode::Dm(dm_node) = &current_room {
                        let profile_sig = store
                            .get_member_profile(&current_room.room_id(), &dm_node.other_user_id);

                        view! {
                            <div class="flex flex-row gap-1 items-center">
                                {move || profile_sig.get().render_name_popup("16px")}
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <div class="flex flex-row gap-1 items-center">
                                {move || current_room.name()}
                            </div>
                        }
                            .into_any()
                    }
                }}
            </div>
            <div class="self-center h-full">
                <button
                    class="transition-opacity h-full aspect-square text-dim"
                    class=("hover:text-(--online-color)", move || !in_call())
                    class=("hover:text-(--error-color)", move || in_call())
                    on:click=on_call_click
                >
                    <div class="h-full justify-center items-center flex cursor-pointer">
                        {move || {
                            let icon = if in_call() { PHONE_DISCONNECT } else { PHONE };
                            view! { <Icon icon=icon size="60%" color="currentColor" /> }
                        }}
                    </div>
                </button>
            </div>
            <div class="self-center h-full">
                <button
                    class="transition-opacity h-full aspect-square text-dim hover:text-(--pin-color)"
                    on:click=on_pin_click
                >
                    <div class="h-full justify-center items-center flex cursor-pointer">
                        <Icon
                            icon=PUSH_PIN
                            size="60%"
                            color="currentColor"
                            weight=move || {
                                if pinned_messages.get().is_some() {
                                    IconWeight::Fill
                                } else {
                                    IconWeight::Light
                                }
                            }
                        />
                    </div>
                </button>
            </div>
            <div class="flex items-center h-full">
                <div class="self-center h-full">
                    <button
                        class="transition-opacity h-full aspect-square text-dim hover:text-(--accent-color)"
                        on:click=move |_| chat_sidebar_open.update(|v| *v = !*v)
                    >
                        <div class="h-full justify-center items-center flex cursor-pointer">
                            {move || {
                                let icon = toggle_icon();
                                view! {
                                    <Icon
                                        icon=icon
                                        size="60%"
                                        color="currentColor"
                                        weight=move || {
                                            if chat_sidebar_open.get() {
                                                IconWeight::Fill
                                            } else {
                                                IconWeight::Light
                                            }
                                        }
                                    />
                                }
                            }}
                        </div>
                    </button>
                </div>
                <div
                    class="ui-solid-bg text-sm h-7 rounded-ui px-1 border w-[200px] text-dim flex transition-colors duration-100"
                    class=("border-(--focus-color)", search_input_focused)
                    class=("border-(--tile-border-color)", move || !search_input_focused())
                >
                    <input
                        node_ref=search_input_ref
                        type="text"
                        placeholder=move || {
                            format!("Search {}...", name().unwrap_or("Room".to_string()))
                        }
                        class="text-normal placeholder:text-muted outline-none flex-1 min-w-0"
                        prop:value=move || search_params.get().map(|p| p.text).unwrap_or_default()
                        on:input=on_search_input
                        on:keydown=on_search_keydown
                    />
                    <button
                        class=("cursor-text", move || input_empty.get())
                        class=("cursor-pointer", move || !input_empty.get())
                        class=("hover:text-normal", move || !input_empty.get())
                        on:click=move |_| {
                            if let Some(input) = search_input_ref.get() {
                                if !input.value().is_empty() {
                                    input.set_value("");
                                    search_params.set(None);
                                    search_results.set(None);
                                    let _ = input.blur();
                                } else {
                                    let _ = input.focus();
                                }
                            }
                        }
                    >
                        {input_icon}
                    </button>
                </div>
                <div class="h-6 w-[2px] mx-2 bg-(--tile-border-color) rounded" />
                <SystemButtons active=false />
            </div>
        </FloatingTile>
    }
}
