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

use crate::components::FloatingTile;
use crate::{
    components::{
        chat::{JumpTarget, messages::render_timeline_item},
        presence::PresenceBadge,
        user_profile::{MemberProfileExt, render_user_profile_card},
    },
    state::{AppState, ProfileStore},
};

enum ChatSidebarContent {
    Search,
    Pins,
    None,
}

impl ChatSidebarContent {
    fn from_memos(search_open: Memo<bool>, pin_open: Memo<bool>) -> Self {
        if search_open.get() {
            ChatSidebarContent::Search
        } else if pin_open.get() {
            ChatSidebarContent::Pins
        } else {
            ChatSidebarContent::None
        }
    }

    fn is_none(&self) -> bool {
        matches!(self, ChatSidebarContent::None)
    }
}

#[component]
pub fn ChatSideBar(chat_sidebar_open: RwSignal<bool>) -> AnyView {
    let search_parameters: RwSignal<Option<SearchParameters>> = expect_context();
    let state: AppState = expect_context();
    let pin_results: RwSignal<Option<Vec<UiTimelineItem>>> = expect_context();

    let search_open = Memo::new(move |_| search_parameters.get().is_some());
    let pin_open = Memo::new(move |_| pin_results.get().is_some());

    let content = move || {
        let content_type = ChatSidebarContent::from_memos(search_open, pin_open);

        if content_type.is_none() && !chat_sidebar_open.get() {
            return ().into_any();
        }

        let (width, content) = match content_type {
            ChatSidebarContent::Search => ("w-[30rem]", chat_search()),
            ChatSidebarContent::Pins => ("w-[30rem]", pinned_messages(pin_results)),
            ChatSidebarContent::None => {
                if chat_sidebar_open.get() {
                    let dm = matches!(state.active_room.get(), Some(RoomNode::Dm(_)));
                    let width = if dm { "w-[20rem]" } else { "w-[15rem]" };

                    (width, chat_info())
                } else {
                    return ().into_any();
                }
            }
        };

        view! {
            <div class=format!("flex-shrink-0 h-full ml-[var(--gap)] {width}")>
                <FloatingTile class="w-full h-full">{content}</FloatingTile>
            </div>
        }
        .into_any()
    };

    view! { {content} }.into_any()
}

fn chat_info() -> AnyView {
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
                        <span class="text-bright">{name_profile.render_name_popup("15px")}</span>
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
                        <span class="text-bright">{name_profile.render_name_popup("15px")}</span>
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

fn chat_search() -> AnyView {
    let state: AppState = expect_context();

    let search_params: RwSignal<Option<SearchParameters>> = expect_context();
    let search_results: RwSignal<Option<HashMap<String, Vec<UiTimelineItem>>>> = expect_context();

    let collapsed_rooms: RwSignal<HashSet<String>> = RwSignal::new(HashSet::new());

    let highlight_words: Memo<Vec<String>> = Memo::new(move |_| {
        search_params
            .get()
            .map(|p| p.text.split_whitespace().map(str::to_lowercase).collect())
            .unwrap_or_default()
    });

    let header_text = move || {
        let Some(results) = search_results.get() else {
            return "Type at least three letters to start searching".to_string();
        };

        let len = results.values().filter(|v| !v.is_empty()).count();
        let total_len: usize = results.values().map(|v| v.len()).sum();

        if total_len == 0 {
            return "No results".to_string();
        }

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

    let room_ids = Memo::new(move |_| {
        let mut ids: Vec<String> = search_results
            .get()
            .map(|results| {
                results
                    .iter()
                    .filter(|(_, messages)| !messages.is_empty())
                    .map(|(room_id, _)| room_id.clone())
                    .collect()
            })
            .unwrap_or_default();

        ids.sort_by_cached_key(|id| {
            let name = state.get_room(id).map(|n| n.name()).unwrap_or_default();
            (name, id.clone())
        });
        ids
    });

    view! {
        <div class="flex flex-col w-full h-full min-h-0 overflow-visible">
            <div class="w-full h-(--header-height) shrink-0 text-normal flex items-center pl-[calc((var(--header-height)-1lh)/2)] border-b border-(--tile-border-color)">
                {header_text}
            </div>
            <div class="flex flex-1 min-h-0 flex-col gap-(--gap) w-full overflow-y-auto p-(--gap)">
                <For
                    each=move || room_ids.get()
                    key=|room_id| room_id.clone()
                    children=move |room_id| room_results(room_id, collapsed_rooms, highlight_words)
                />
            </div>
        </div>
    }
    .into_any()
}

fn room_results(
    room_id: String,
    collapsed_rooms: RwSignal<HashSet<String>>,
    highlight_words: Memo<Vec<String>>,
) -> AnyView {
    let state: AppState = expect_context();
    let search_results: RwSignal<Option<HashMap<String, Vec<UiTimelineItem>>>> = expect_context();

    let Some(node) = state.get_room(&room_id) else {
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

    let messages = Memo::new({
        let room_id = room_id.clone();
        move |_| {
            search_results
                .get()
                .and_then(|results| results.get(&room_id).cloned())
                .unwrap_or_default()
        }
    });

    let is_collapsed = Memo::new({
        let room_id = room_id.clone();
        move |_| collapsed_rooms.get().contains(&room_id)
    });

    let toggle_room_id = room_id.clone();

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
                    {move || {
                        let icon = if is_collapsed.get() { CARET_UP } else { CARET_DOWN };
                        view! { <Icon icon=icon size="20px" /> }
                    }}
                </button>
            </div>
            {move || {
                if is_collapsed.get() {
                    return ().into_any();
                }
                let room_id = room_id.clone();

                view! {
                    <For
                        each=move || messages.get()
                        key=|msg| msg.render_key()
                        children=move |msg| message_result(msg, room_id.clone(), highlight_words)
                    />
                }
                    .into_any()
            }}
        </div>
    }
    .into_any()
}

fn message_result(
    msg: UiTimelineItem,
    room_id: String,
    highlight_words: Memo<Vec<String>>,
) -> AnyView {
    let state: AppState = expect_context();
    let JumpTarget(jump_target) = expect_context();

    let event_id = msg.event_id();
    let highlighted = Memo::new(move |_| msg.with_highlights(&highlight_words.get()));

    let hovered = RwSignal::new(false);

    view! {
        <button
            class="relative p-1 bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--gap) cursor-pointer items-start text-left"
            on:mouseenter=move |_| hovered.set(true)
            on:mouseleave=move |_| hovered.set(false)
            on:click=move |_| {
                let Some(event_id) = event_id.clone() else {
                    return;
                };
                jump_target.set(Some(event_id));
                state.set_active_room_with_id(Some(room_id.clone()));
            }
        >
            {move || {
                render_timeline_item(
                    RwSignal::new(highlighted.get()),
                    true,
                    true,
                    Callback::new(move |_| {}),
                    RwSignal::new(None),
                )
            }}
            <div
                class="absolute top-2 right-2 text-xs text-dim bg-white/8 rounded-(--gap) px-1 py-0.5"
                class=("opacity-100", move || hovered.get())
                class=("opacity-0", move || !hovered.get())
            >
                "Jump"
            </div>
        </button>
    }
    .into_any()
}

fn pinned_messages(pinned_result: RwSignal<Option<Vec<UiTimelineItem>>>) -> AnyView {
    let state: AppState = expect_context();

    let content = move || {
        let Some(room_id) = state.active_room_id_untracked() else {
            return view! { <div class="flex-1 flex items-center justify-center text-muted">"No pinned messages"</div> }
            .into_any();
        };

        let Some(messages) = pinned_result.get() else {
            return view! { <div class="flex-1 flex items-center justify-center text-muted">"No pinned messages"</div> }
            .into_any();
        };

        if messages.is_empty() {
            return view! { <div class="flex-1 flex items-center justify-center text-muted">"No pinned messages"</div> }
            .into_any();
        }

        view! {
                <div class="flex flex-1 min-h-0 flex-col gap-(--gap) w-full overflow-y-auto p-(--gap)">
                    <For
                        each=move || messages.clone()
                        key=|msg| msg.render_key()
                        children=move |msg| message_result(
                            msg,
                            room_id.clone(),
                            Memo::new(|_| vec![]),
                        )
                    />
                </div>
        }.into_any()
    };

    let text = move || {
        let len = pinned_result.get().map(|r| r.len()).unwrap_or(0);

        if len == 0 {
            "No pinned messages".to_string()
        } else {
            format!("Pinned Messages ({})", len)
        }
    };

    view! {
        <div class="flex flex-col gap-(--gap) w-full h-full min-h-0 overflow-visible">

        <div class="w-full h-(--header-height) shrink-0 text-normal flex items-center pl-[calc((var(--header-height)-1lh)/2)] border-b border-(--tile-border-color)">
            {text}
        </div>
        {content}
        </div>
    }.into_any()
}
