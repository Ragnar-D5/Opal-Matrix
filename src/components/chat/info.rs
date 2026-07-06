use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use phosphor_leptos::Icon;
use phosphor_leptos::{AT, CARET_DOWN, CARET_UP, GLOBE, HASH, QUESTION_MARK, SPEAKER_HIGH};
use shared::{
    api::SearchParameters,
    profile::{MemberProfile, PresenceInfo},
    sidebar::RoomNode,
    timeline::UiTimelineItem,
};

use crate::{
    components::{
        chat::messages::render_timeline_item,
        presence::PresenceBadge,
        user_profile::{render_user_profile_card, MemberProfileExt},
    },
    state::{AppState, ProfileStore},
};

pub fn chat_info() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let content = move || {
        let Some(node) = state.active_room.get() else {
            return ().into_any();
        };

        match &node {
            RoomNode::Dm(dm_node) => {
                let profile_sig = store.get_member_profile(&node.room_id(), &dm_node.other_user_id);

                let member = profile_sig.get_untracked();
                let user_id = member.profile.user_id.clone();
                let room_id = member.room_id.clone();
                render_user_profile_card(70.0, 108.0, user_id, Some(room_id)).into_any()
            }
            RoomNode::TextChannel(_)
            | RoomNode::Single(_)
            | RoomNode::VoiceChannel(_)
            | RoomNode::Server(_)
            | RoomNode::Space(_) => member_list(),
            RoomNode::Unjoined(_) => view! {
                <div class="flex-1 flex items-center justify-center text-muted">
                    "No information available for this room"
                </div>
            }
            .into_any(),
        }
    };

    view! { <div class="flex flex-col w-full h-full min-h-0 overflow-visible">{content}</div> }
        .into_any()
}

fn member_list() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let room_id = state.active_room_id_untracked().unwrap_or_default();

    let members_store = store.clone();
    let members = Memo::new(move |_| members_store.clone().get_member_signals(&room_id));

    let online_store = store.clone();
    let online_view = move || {
        let members = members.get();

        let mut elements: Vec<(
            String,
            ArcRwSignal<MemberProfile>,
            ArcRwSignal<PresenceInfo>,
        )> = members
            .into_iter()
            .filter_map(|(user_id, member_sig)| {
                let presence = online_store.get_presence(&user_id);

                if !presence.get().is_offline() {
                    let name = member_sig.get().get_name();
                    Some((name, member_sig, presence))
                } else {
                    None
                }
            })
            .collect();

        elements.sort_by_key(|v| v.0.clone());

        let views: Vec<_> = elements
            .into_iter()
            .map(|(_, member_sig, presence)| {
                let profile = member_sig.get();
                let name_profile = profile.clone();

                view! {
                    <div class="flex items-center gap-2">
                        <PresenceBadge presence=presence size=15.5>
                            {profile.render_icon("32px")}
                        </PresenceBadge>
                        <span class="text-bright">{name_profile.render_name_popup("16px")}</span>
                    </div>
                }
            })
            .collect();

        let online_i = views.len();

        let number_view =
            view! { <span class="text-sm text-muted">{format!("Online — {online_i}")}</span> }
                .into_any();

        if online_i > 0 {
            view! {
                <div>
                    {number_view} <div class="flex flex-col gap-2 mt-2">{views.collect_view()}</div>
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        }
    };

    let offline_store = store.clone();
    let offline_view = move || {
        let members = members.get();

        let mut elements: Vec<(
            String,
            ArcRwSignal<MemberProfile>,
            ArcRwSignal<PresenceInfo>,
        )> = members
            .into_iter()
            .filter_map(|(user_id, member_sig)| {
                let presence = offline_store.get_presence(&user_id);

                if presence.get().is_offline() {
                    let name = member_sig.get().get_name();
                    Some((name, member_sig, presence))
                } else {
                    None
                }
            })
            .collect();

        elements.sort_by_key(|v| v.0.clone());

        let views: Vec<_> = elements
            .into_iter()
            .map(|(_, member_sig, presence)| {
                let profile = member_sig.get();
                let name_profile = profile.clone();

                view! {
                    <div class="flex items-center gap-2">
                        <PresenceBadge presence=presence size=15.5>
                            {profile.render_icon("32px")}
                        </PresenceBadge>
                        <span class="text-bright">{name_profile.render_name_popup("18px")}</span>
                    </div>
                }
            })
            .collect();

        let offline_i = views.len();

        let number_view =
            view! { <span class="text-sm text-muted">{format!("Offline — {offline_i}")}</span> }
                .into_any();

        if offline_i > 0 {
            view! {
                <div>
                    {number_view} <div class="flex flex-col gap-2 mt-2">{views.collect_view()}</div>
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        }
    };

    let header = move || {
        let members = members.get();

        let mut online_count = 0;
        let mut offline_count = 0;

        for member in members.keys() {
            let presence = store.get_presence(member);
            if !presence.get().is_offline() {
                online_count += 1;
            } else {
                offline_count += 1;
            }
        }

        view! {
            <div class="flex items-center gap-2 justify-center">
                <div class="w-3 h-3 rounded-full bg-(--online-color)"></div>
                <span class="text-ms text-(--online-color) pr-5">{online_count}</span>
                <div class="w-3 h-3 rounded-full bg-(--offline-color)"></div>
                <span class="text-ms text-(--offline-color)">{offline_count}</span>
            </div>
        }
    };

    view! { <div class="flex flex-col gap-2 p-3">{header} {online_view} {offline_view}</div> }
        .into_any()
}

pub fn chat_search() -> AnyView {
    let state: AppState = expect_context();

    let search_results: RwSignal<Option<HashMap<String, Vec<UiTimelineItem>>>> = expect_context();

    let collapsed_rooms: RwSignal<HashSet<String>> = RwSignal::new(HashSet::new());

    let header_text = move || {
        let Some(results) = search_results.get() else {
            return "Search".to_string();
        };

        let len = results.len();
        let total_len: usize = results.values().map(|v| v.len()).sum();

        let extra = if len > 1 {
            Some(format!(" across {} rooms", len))
        } else {
            None
        };

        format!(
            "{} Result{}{}",
            total_len,
            if total_len == 1 { "" } else { "s" },
            extra.unwrap_or_default()
        )
    };

    let content = move || {
        let Some(results) = search_results.get() else {
            return ().into_any();
        };

        let collapsed = collapsed_rooms.get();

        results
            .iter()
            .map(|(room_id, messages)| {
                let Some(node) = state.get_room(room_id) else {
                    return ().into_any();
                };

                let room_name = node.name();

                let icon = match node {
                    RoomNode::Dm(_) => AT,
                    RoomNode::TextChannel(_) | RoomNode::Single(_) => HASH,
                    RoomNode::Server(_) | RoomNode::Space(_) => GLOBE,
                    RoomNode::VoiceChannel(_) => SPEAKER_HIGH,
                    RoomNode::Unjoined(_) => QUESTION_MARK,
                };

                let is_collapsed = collapsed.contains(room_id);
                let toggle_room_id = room_id.clone();

                let messages_view = if is_collapsed {
                    ().into_any()
                } else {
                    render_list_of_messages(messages)
                };

                view! {
                    <div class="flex flex-col gap-(--gap) text-normal">
                        <div class="flex flex-row items-center gap-1 p-2">
                            <div
                                class="flex flex-row flex-1 text-normal cursor-pointer gap-1 items-center hover:text-(--accent-color) hover:underline"

                                on:click=move |_| {
                                    state.set_active_room_with_id(Some(node.room_id()));
                                }
                            >
                                <Icon icon=icon size="20px" />
                                <span>{room_name}</span>
                            </div>
                            <button
                                class="text-muted hover:text-bright cursor-pointer flex items-center justify-center"
                                on:click=move |_| {
                                    collapsed_rooms
                                        .update(|rooms| {
                                            if !rooms.remove(&toggle_room_id) {
                                                rooms.insert(toggle_room_id.clone());
                                            }
                                        });
                                }
                            >
                                <Icon
                                    icon=if is_collapsed { CARET_UP } else { CARET_DOWN }
                                    size="20px"
                                />
                            </button>
                        </div>
                        {messages_view}
                    </div>
                }
                .into_any()
            })
            .collect_view()
            .into_any()
    };

    view! {
        <div class="flex flex-col w-full h-full min-h-0 overflow-visible">
            <div class="w-full h-(--header-height) shrink-0 text-normal flex items-center pl-[calc((var(--header-height)-1lh)/2)] border-b border-(--tile-border-color)">
                {header_text}
            </div>
            <div class="flex flex-1 min-h-0 flex-col gap-(--gap) w-full overflow-y-auto p-(--gap)">
                {content}
            </div>
        </div>
    }
    .into_any()
}

fn render_list_of_messages(messages: &[UiTimelineItem]) -> AnyView {
    let render_msg = move |msg: &UiTimelineItem| {
        let content = render_timeline_item(
            RwSignal::new(msg.clone()),
            true,
            true,
            Callback::new(move |_| {}),
        );

        view! {
            <div class="p-1 bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--gap)">
                {content}
            </div>
        }
    };

    messages.iter().map(render_msg).collect_view().into_any()
}
