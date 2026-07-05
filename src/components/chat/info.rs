use leptos::prelude::*;
use shared::{
    api::RoomSearchParameters,
    profile::{MemberProfile, PresenceInfo},
    sidebar::RoomNode,
};

use crate::{
    components::{
        presence::PresenceBadge,
        user_profile::{render_user_profile_card, MemberProfileExt},
    },
    state::{AppState, ProfileStore},
};

#[component]
pub fn ChatInfo() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let search_params: RwSignal<Option<RoomSearchParameters>> = expect_context();

    let content = move || {
        let Some(node) = state.active_room.get() else {
            return ().into_any();
        };

        if search_params.get().is_some() {
            return view! { <Search /> }.into_any();
        }

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

    view! { <div class="flex flex-col w-full overflow-visible">{content}</div> }.into_any()
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

#[component]
pub fn Search() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let search_params: RwSignal<Option<RoomSearchParameters>> = expect_context();

    view! { <div class="flex flex-col w-full overflow-visible">Testing</div> }.into_any()
}
