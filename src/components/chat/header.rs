use std::collections::HashMap;

use leptos::{html::Input, prelude::*, task::spawn_local};
use phosphor_leptos::{
    HASH, INFO, Icon, IconWeight, MAGNIFYING_GLASS, MATRIX_LOGO, PHONE, PHONE_DISCONNECT, SPEAKER_HIGH, USER_CIRCLE, USER_LIST, X
};
use shared::{api::SearchParameters, sidebar::RoomNode, timeline::UiTimelineItem};
use uuid::Uuid;

use crate::{
    app::call_tauri,
    components::{presence::PresenceBadge, user_profile::MemberProfileExt, FloatingTile},
    state::{AppState, CurrentSection, ProfileStore},
    tauri_functions::search_rooms,
};

#[component]
pub fn ChatHeader(chat_sidebar_open: RwSignal<bool>) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let info_hovered = RwSignal::new(false);

    let toggle_icon = move || {
        if let Some(node) = state.active_room.get() {
            if node.is_dm() {
                USER_CIRCLE
            } else if node.is_space() {
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
    let search_results: RwSignal<Option<HashMap<String, Vec<UiTimelineItem>>>> = expect_context();

    let on_search_input = move |ev| {
        let input = event_target_value(&ev);

        let mut params = search_params.get().unwrap_or_default();
        params.text = input.clone();

        let active_room_id = state.active_room_id();

        if params.room_ids.is_empty() && let Some(room_id) = active_room_id.clone() {
            params.room_ids.push(room_id);
        }

        if params.is_empty(active_room_id.clone()) {
            search_params.set(None);
            search_results.set(None);

            if let Some(room_id) = active_room_id {
                state.room_states.update(|drafts| {
                    let draft = drafts.entry(room_id).or_default();
                    draft.search_parameters = None;
                    draft.search_results = None;
                });
            }
            return;
        }

        if input.len() < 3 {
            return;
        }

        let search_id = Uuid::new_v4();
        params.search_id = search_id;

        let results = Some(HashMap::new());
        search_results.set(results.clone());
        search_params.set(Some(params.clone()));

        if let Some(room_id) = active_room_id {
            state.room_states.update(|drafts| {
                let draft = drafts.entry(room_id).or_default();
                draft.search_parameters = Some(params.clone());
                draft.search_results = results;
            });
        }

        search_rooms(params, search_id);
    };

    let search_input_ref: NodeRef<Input> = NodeRef::new();
    let input_empty = Memo::new(move |_| {
        search_params
            .get()
            .map(|p| p.text.is_empty()).unwrap_or(true)
    });

    let input_icon = move || {
        let icon = if input_empty.get() {
            MAGNIFYING_GLASS
        } else {
            X
        };

        view! { <Icon icon=icon size="16px" /> }
    };

    let store_clone = store.clone();
    view! {
        <FloatingTile class="h-(--header-height) items-start flex-row gap-1 pl-[5px]">
            <div class="w-8 self-center flex items-center justify-center">
                {move || {
                    let clone = store.clone();
                    let Some(node) = state.active_room.get() else {
                        return view! {
                            <div class="text-(--ui-base-color) w-full justify-center flex">
                                <Icon icon=INFO color="currentColor" size="70%" />
                            </div>
                        }
                            .into_any();
                    };
                    match &node {
                        RoomNode::TextChannel(_) | RoomNode::Single(_) => {

                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=HASH color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                        RoomNode::VoiceChannel(_) => {
                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=SPEAKER_HIGH color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                        RoomNode::Dm(dm_node) => {
                            let profile_sig = store
                                .get_member_profile(&node.room_id(), &dm_node.other_user_id);
                            {

                                view! {
                                    {move || {
                                        let profile = profile_sig.get();
                                        let presence = clone
                                            .clone()
                                            .get_presence(profile.user_id());
                                        view! {
                                            <PresenceBadge presence=presence size=14.0>
                                                {profile.render_icon("30px")}
                                            </PresenceBadge>
                                        }
                                            .into_any()
                                    }}
                                }
                            }
                                .into_any()
                        }
                        RoomNode::Unjoined(_) => {
                            view! {
                                <div class="w-8 text-end">
                                    <span class="text-lg text-bright self-center align-middle">
                                        "?"
                                    </span>
                                </div>
                            }
                                .into_any()
                        }
                        RoomNode::Server(_) | RoomNode::Space(_) => {
                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=MATRIX_LOGO color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                    }
                }}
            </div>
            <div class="flex-1 flex flex-col self-center text-bright text-m font-semibold">
                {move || {
                    let Some(current_room) = state.active_room.get() else {
                        return view! { <span>"No Room"</span> }.into_any();
                    };
                    if let RoomNode::Dm(dm_node) = &current_room {
                        let profile_sig = store_clone
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
            <div class="flex items-center h-full pr-[90px]">
                <div class="self-center h-full">
                    <button
                        class="transition-opacity h-full aspect-square"
                        class=("text-(--ui-hover-color)", move || info_hovered.get())
                        class=("text-(--ui-base-color)", move || !info_hovered.get())
                        on:click=move |_| chat_sidebar_open.update(|v| *v = !*v)
                        on:mouseenter=move |_| info_hovered.set(true)
                        on:mouseleave=move |_| info_hovered.set(false)
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
                <div class="self-center h-full">
                    <button
                        class="transition-opacity h-full aspect-square"
                        class=("text-(--ui-hover-color)", move || info_hovered.get())
                        class=("text-(--ui-base-color)", move || !info_hovered.get())
                        on:click=move |_| {
                            let value = serde_wasm_bindgen::to_value(
                                &serde_json::json!({"room_id": &state.active_room_id().unwrap()}),
                            );
                            spawn_local(async move {
                                log::debug!(
                                    "{:?}", call_tauri("join_matrixrtc_call", value.unwrap()).await
                                );
                            })
                        }
                        on:mouseenter=move |_| info_hovered.set(true)
                        on:mouseleave=move |_| info_hovered.set(false)
                    >
                        <div class="h-full justify-center items-center flex cursor-pointer">
                            <Icon
                                icon=PHONE
                                size="60%"
                                color="currentColor"
                                weight=IconWeight::Duotone
                            />
                        </div>
                    </button>
                </div>
                <div class="self-center h-full">
                    <button
                        class="transition-opacity h-full aspect-square mr-1"
                        class=("text-(--ui-hover-color)", move || info_hovered.get())
                        class=("text-(--ui-base-color)", move || !info_hovered.get())
                        on:click=move |_| {
                            let value = serde_wasm_bindgen::to_value(
                                &serde_json::json!({"room_id": &state.active_room_id().unwrap()}),
                            );
                            spawn_local(async move {
                                log::debug!(
                                    "{:?}", call_tauri("leave_matrixrtc_call", value.unwrap()).await
                                );
                            })
                        }
                        on:mouseenter=move |_| info_hovered.set(true)
                        on:mouseleave=move |_| info_hovered.set(false)
                    >
                        <div class="h-full justify-center items-center flex cursor-pointer">
                            <Icon
                                icon=PHONE_DISCONNECT
                                size="60%"
                                color="currentColor"
                                weight=IconWeight::Duotone
                            />
                        </div>
                    </button>
                </div>
                <div class="bg-(--ui-solid-bg) text-sm h-7 rounded-(--ui-border-radius) px-1 border border-(--tile-border-color) w-[200px] text-dim flex">
                    <input
                        node_ref=search_input_ref
                        type="text"
                        placeholder=move || {
                            format!("Search {}...", name().unwrap_or("Room".to_string()))
                        }
                        class="text-normal placeholder:text-muted outline-none flex-1 min-w-0"
                        prop:value=move || search_params.get().map(|p| p.text).unwrap_or_default()
                        on:input=on_search_input
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
                <div class="h-6 w-[2px] ml-2 mx-1 bg-(--tile-border-color) rounded" />
            </div>
        </FloatingTile>
    }
}
