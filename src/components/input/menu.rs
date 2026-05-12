use crate::{
    components::{
        input::{get_caret_position, get_node_and_offset},
        presence::PresenceBadge,
        RichTextExt,
    },
    state::{AppState, MemberStore},
    tauri_functions::{get_members, MemberShip},
};
use leptos::html::Div;
use leptos::prelude::*;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use web_sys::HtmlElement;

use crate::components::user_profile::UserProfileMaybeExt;

#[derive(Clone, PartialEq, Debug)]
pub enum MenuType {
    None,
    Mentions { filter: String },
    // Commands { filter: String },
}

impl MenuType {
    fn is_none(&self) -> bool {
        matches!(self, MenuType::None)
    }
}

pub fn commit_mention(
    el: &HtmlElement,
    membership: &MemberShip,
    state: AppState,
    store: MemberStore,
) {
    let doc = document();
    let caret_pos = get_caret_position(el);

    let text = el.text_content().unwrap_or_default();
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let offset = caret_pos as usize;

    let mut start_idx = offset;
    while start_idx > 0 {
        let prev = utf16[start_idx - 1];
        if prev == 32 || prev == 160 || prev == 10 {
            break;
        }
        start_idx -= 1;
    }

    let mut end_idx = offset;
    while end_idx < utf16.len() {
        let next = utf16[end_idx];
        if next == 32 || next == 160 || next == 10 {
            break;
        }
        end_idx += 1;
    }

    let start_pos = start_idx as u32;
    let end_pos = end_idx as u32;

    let start_loc = get_node_and_offset(el, start_pos);
    let end_loc = get_node_and_offset(el, end_pos);

    if let (Some((start_node, start_off)), Some((end_node, end_off))) = (start_loc, end_loc) {
        let range = doc.create_range().unwrap();
        range.set_start(&start_node, start_off).unwrap();
        range.set_end(&end_node, end_off).unwrap();

        range.delete_contents().unwrap();

        let room_id = state.active_room_id.get_untracked().unwrap_or_default();

        let mention_view = membership.to_span(state.clone()).render(store, room_id);

        let any_view: AnyView = mention_view.into_any();

        let mut state = any_view.build();

        let temp_container = doc.create_element("div").unwrap();
        state.mount(&temp_container, None);

        let mention_node = temp_container
            .first_child()
            .expect("Mention view should have at least one root element");

        let space_node = doc.create_text_node("\u{00A0}");

        range.insert_node(&space_node).unwrap();
        range.insert_node(&mention_node).unwrap();

        let new_range = doc.create_range().unwrap();
        new_range.set_start(&space_node, 1).unwrap();
        new_range.collapse_with_to_start(true);

        let win = window();
        let sel = win.get_selection().unwrap().unwrap();
        sel.remove_all_ranges().unwrap();
        sel.add_range(&new_range).unwrap();
    }
}

fn filter_mentions(
    filter: String,
    members: Vec<MemberShip>,
    matcher: StoredValue<Matcher>,
) -> Vec<MemberShip> {
    if filter.is_empty() {
        return members;
    }

    let filter = filter.to_lowercase();

    let mut needle_buf = Vec::new();
    let mut haystack_buf = Vec::new();

    let mut matcher = matcher.get_value();
    let needle = Utf32Str::new(filter.as_str(), &mut needle_buf);

    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    for member in members {
        if filter.is_empty() {
            matched.push((0, member));
            continue;
        }

        let mut best_score: Option<u16> = None;

        // --- 1. Match against Display Name (Priority) ---
        if let Some(ref name) = member.display_name {
            let name = name.to_lowercase();

            let haystack = Utf32Str::new(name.as_str(), &mut haystack_buf);
            if let Some(score) = matcher.fuzzy_match(haystack, needle) {
                // Small "bonus" to display name matches so they match higher than user ID matches with the same score
                best_score = Some(score + 10);
            }
        }

        // --- 2. Match against User ID (Fallback/Secondary) ---
        let user_id = member.user_id.to_lowercase();

        let id_haystack = Utf32Str::new(user_id.as_str(), &mut haystack_buf);
        if let Some(id_score) = matcher.fuzzy_match(id_haystack, needle) {
            if best_score.map_or(true, |current| id_score > current) {
                best_score = Some(id_score);
            }
        }

        // --- 3. Categorize ---
        if let Some(score) = best_score {
            matched.push((score, member));
        } else {
            unmatched.push(member);
        }
    }

    matched.sort_by(|a, b| b.0.cmp(&a.0));

    matched.into_iter().map(|(_, m)| m).collect()
}

#[component]
pub fn SelectionMenu(
    menu: RwSignal<MenuType>,
    selected_index: RwSignal<usize>,
    matches: RwSignal<Vec<MemberShip>>,
    input_ref: NodeRef<Div>,
) -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let matcher = StoredValue::new(Matcher::new(Config::DEFAULT));

    let members_resource = LocalResource::new(move || {
        let room_id = state.active_room_id.get();
        async move {
            if let Some(id) = room_id {
                let mut res = get_members(id.clone()).await.unwrap_or_default();
                res.insert(0, MemberShip::room(id));
                res.sort_by(|a, b| a.get_name().cmp(&b.get_name()));

                res
            } else {
                Vec::new()
            }
        }
    });

    Effect::new(move |_| {
        if let MenuType::Mentions { filter, .. } = menu.get() {
            matches.set(filter_mentions(
                filter,
                members_resource.get().unwrap_or_default(),
                matcher,
            ));
        }
    });

    view! {
        <div
            class="absolute bottom-full left-4 right-4 bottom-(--gap) bg-(--ui-floating-bg) backdrop-blur-2xl rounded-(--ui-border-radius) border border-(--tile-border-color) flex flex-col text-xs pb-(--gap)"
            class:hidden=move || menu.get().is_none()
        >
            {move || {
                let Some(el) = input_ref.get() else {
                    return view! {}.into_any();
                };
                let store = store.clone();
                let room_id = state.active_room_id.get().unwrap_or_default();
                match menu.get() {
                    MenuType::Mentions { filter, .. } => {
                        view! {
                            <span class="text-(--ui-base-color) bold text-xs p-2 bb-4">
                                {
                                    let len = matches.get().len();
                                    format!(
                                        "MEMBERS {}",
                                        if filter.is_empty() {
                                            format!("({len})")
                                        } else {
                                            format!("MATCHING @{filter} ({len})")
                                        },
                                    )
                                }
                            </span>
                            <For
                                each=move || matches.get().into_iter().enumerate()
                                key=|(_, member)| member.user_id.clone()
                                children=move |(idx, member)| {
                                    let presence = store.get_presence(&member.user_id);
                                    let profile = store
                                        .get_profile(&room_id, &member.user_id)
                                        .get();
                                    let p_clone = profile.clone();
                                    let m_clone = member.clone();
                                    let el = el.clone();
                                    let store = store.clone();

                                    view! {
                                        <button
                                            class="flex flex-row items-center gap-2 mx-(--gap) px-(--gap) py-1 rounded-(--ui-border-radius) cursor-pointer"
                                            class=(
                                                "bg-(--ui-hover-bg)",
                                                move || idx == selected_index.get(),
                                            )
                                            on:mouseenter=move |_| selected_index.set(idx)
                                            on:click=move |_| {
                                                commit_mention(&el.clone(), &member, state, store.clone())
                                            }
                                        >
                                            {move || {
                                                let profile = profile.clone();
                                                let presence = presence.clone();
                                                if state.active_room_id.get().unwrap_or_default()
                                                    != m_clone.user_id
                                                {
                                                    view! {
                                                        <PresenceBadge presence=presence size=15.0>
                                                            {profile.render_icon(30)}
                                                        </PresenceBadge>
                                                    }
                                                        .into_any()
                                                } else {
                                                    profile.render_icon(30).into_any()
                                                }
                                            }}
                                            {p_clone.render_name(14)}
                                            <div class="flex flex-grow"></div>
                                            <span
                                                class=(
                                                    "text-(--ui-hover-color)",
                                                    move || idx == selected_index.get(),
                                                )
                                                class=(
                                                    "text-(--ui-base-color)",
                                                    move || idx != selected_index.get(),
                                                )
                                            >
                                                {member.user_id.clone()}
                                            </span>
                                        </button>
                                    }
                                }
                            />
                        }
                            .into_any()
                    }
                    _ => view! {}.into_any(),
                }
            }}
        </div>
    }.into_any()
}
