use crate::{
    components::{
        input::{get_caret_position, get_node_and_offset},
        presence::PresenceBadge,
        text::RichTextExt,
        user_profile::{room_as_profile, MemberProfileExt},
    },
    state::{AppState, ProfileStore},
    tauri_functions::{get_commands, get_members_for_room},
};
use leptos::html::Div;
use leptos::prelude::*;
use log::error;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use shared::{commands::Command, user_profile::MemberProfile};
use web_sys::{Document, HtmlDivElement, HtmlElement, Node, Range};

#[derive(Clone, PartialEq, Debug)]
pub enum MenuType {
    None,
    UserAutocomplete { filter: String },
    CommandAutocomplete { filter: String },
}

impl MenuType {
    fn is_none(&self) -> bool {
        matches!(self, MenuType::None)
    }
}

pub enum SelectedItem {
    User(MemberProfile),
    Command(Command),
}

impl From<MemberProfile> for SelectedItem {
    fn from(profile: MemberProfile) -> Self {
        SelectedItem::User(profile)
    }
}

impl From<Command> for SelectedItem {
    fn from(command: Command) -> Self {
        SelectedItem::Command(command)
    }
}

fn render_command(command: Command) -> impl IntoView {
    if command.is_macro().is_some() {
        return ().into_any();
    };

    view! {
        <span
            contenteditable="false"
            class="rounded-(--floating-border-radius) bg-(--ui-solid-bg) border border-(--accent-color) px-[2px]"
        >
            /
            {command.name}
        </span>
    }
    .into_any()
}

fn commit_command(command: Command, doc: &Document, range: Range) -> (Node, u32) {
    if let Some((replacement, caret_position)) = &command.is_macro() {
        let text_node = doc.create_text_node(replacement);
        let text_len = text_node.length();
        range.insert_node(&text_node).unwrap();
        return (Node::from(text_node), caret_position.unwrap_or(text_len));
    }

    let mut render_state = render_command(command).build();
    let temp_container = doc.create_element("div").unwrap();
    render_state.mount(&temp_container, None);

    let command_node = temp_container
        .first_child()
        .expect("Command view should have at least one root element");

    let space_node = doc.create_text_node("\u{00A0}");

    range.insert_node(&space_node).unwrap();
    range.insert_node(&command_node).unwrap();

    (web_sys::Node::from(space_node), 1)
}

pub fn commit_selection(
    el: &HtmlElement,
    selected: SelectedItem,
    state: AppState,
    store: ProfileStore,
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

    let (Some((start_node, start_off)), Some((end_node, end_off))) = (start_loc, end_loc) else {
        return;
    };

    let range = doc.create_range().unwrap();
    range.set_start(&start_node, start_off).unwrap();
    range.set_end(&end_node, end_off).unwrap();

    range.delete_contents().unwrap();

    let (focus_node, focus_offset) = match selected {
        SelectedItem::User(membership) => {
            let room_id = state.active_room_id_untracked().unwrap_or_default();
            let mention_view = membership.to_span().render(store, room_id);
            let any_view: AnyView = mention_view.into_any();
            let mut render_state = any_view.build();

            let temp_container = doc.create_element("div").unwrap();
            render_state.mount(&temp_container, None);

            let mention_node = temp_container
                .first_child()
                .expect("Mention view should have at least one root element");

            let space_node = doc.create_text_node("\u{00A0}");

            range.insert_node(&space_node).unwrap();
            range.insert_node(&mention_node).unwrap();

            (web_sys::Node::from(space_node), 1)
        }
        SelectedItem::Command(command) => commit_command(command, &doc, range),
    };

    let new_range = doc.create_range().unwrap();
    new_range.set_start(&focus_node, focus_offset).unwrap();
    new_range.collapse_with_to_start(true);

    let win = window();
    let sel = win.get_selection().unwrap().unwrap();
    sel.remove_all_ranges().unwrap();
    sel.add_range(&new_range).unwrap();
}

fn filter_items<T: RenderMenuRow>(filter: String, items: Vec<T>, matcher: &mut Matcher) -> Vec<T> {
    if filter.is_empty() {
        return items;
    }

    let filter = filter.to_lowercase();
    let mut needle_buf = Vec::new();
    let mut haystack_buf = Vec::new();
    let needle = Utf32Str::new(filter.as_str(), &mut needle_buf);

    let fields = T::match_fields();
    let mut matched = Vec::new();

    for item in items {
        let mut best_score: Option<u16> = None;

        // Cascade through each field function sequentially
        for (idx, field_fn) in fields.iter().enumerate() {
            if let Some(text) = field_fn(&item) {
                let text = text.to_lowercase();
                let haystack = Utf32Str::new(text.as_str(), &mut haystack_buf);

                if let Some(score) = matcher.fuzzy_match(haystack, needle) {
                    // Dynamic Priority Bonus: earlier fields in the vector yield higher scores
                    let bonus = ((fields.len() - 1 - idx) * 10) as u16;
                    let final_score = score + bonus;

                    if best_score.is_none_or(|current| final_score > current) {
                        best_score = Some(final_score);
                    }
                }
            }
        }

        if let Some(score) = best_score {
            matched.push((score, item));
        }
    }

    matched.sort_by_key(|b| std::cmp::Reverse(b.0));
    matched.into_iter().map(|(_, item)| item).collect()
}

trait RenderMenuRow {
    fn id(&self) -> String;
    fn render_row(self, room_id: String, el: HtmlDivElement, idx: usize) -> impl IntoView;
    fn match_fields() -> Vec<fn(&Self) -> Option<String>>;
}

impl RenderMenuRow for MemberProfile {
    fn id(&self) -> String {
        self.user_id().to_string()
    }

    fn render_row(self, room_id: String, el: HtmlDivElement, idx: usize) -> impl IntoView {
        let state: AppState = expect_context();
        let store: ProfileStore = expect_context();
        let selected_index: RwSignal<usize> = expect_context();

        let presence = store.get_presence(self.user_id());
        let profile = store.get_member_profile(&room_id, self.user_id()).get();
        let p_clone = profile.clone();
        let m_clone = self.clone();
        let el = el.clone();
        let store = store.clone();

        view! {
            <button
                class="flex flex-row items-center gap-2 mx-(--gap) px-(--gap) py-1 rounded-(--ui-border-radius) cursor-pointer"
                class=("bg-(--ui-hover-bg)", move || idx == selected_index.get())
                on:mouseenter=move |_| selected_index.set(idx)
                on:click=move |_| {
                    commit_selection(
                        &el.clone(),
                        SelectedItem::from(self.clone()),
                        state,
                        store.clone(),
                    )
                }
            >
                {move || {
                    let profile = profile.clone();
                    let presence = presence.clone();
                    if state.active_room_id().unwrap_or_default() != m_clone.user_id() {
                        view! {
                            <PresenceBadge presence=presence size=15.0>
                                {profile.render_icon("30px")}
                            </PresenceBadge>
                        }
                            .into_any()
                    } else {
                        profile.render_icon("30px").into_any()
                    }
                }}
                {p_clone.render_name("14px")}
                <div class="flex flex-grow"></div>
                <span
                    class=("text-(--ui-hover-color)", move || idx == selected_index.get())
                    class=("text-(--ui-base-color)", move || idx != selected_index.get())
                >
                    {self.user_id().to_string()}
                </span>
            </button>
        }
    }

    fn match_fields() -> Vec<fn(&Self) -> Option<String>> {
        vec![|member| member.profile.display_name.clone(), |member| {
            Some(member.user_id().to_string())
        }]
    }
}

impl RenderMenuRow for Command {
    fn id(&self) -> String {
        format!("{}:{}", self.source, self.name)
    }

    fn render_row(self, _: String, _: HtmlDivElement, idx: usize) -> impl IntoView {
        let selected_index: RwSignal<usize> = expect_context();

        view! {
            <button
                class="flex flex-row justify-center items-center rounded-(--ui-border-radius) cursor-pointer mx-(--gap) px-(--gap) py-1"
                class=("bg-(--ui-hover-bg)", move || selected_index.get() == idx)
            >
                <div class="flex flex-col">
                    <span class="text-start text-sm text-normal">{self.usage}</span>
                    <span
                        class="text-start"
                        class=("text-(--ui-hover-color)", move || idx == selected_index.get())
                        class=("text-(--ui-base-color)", move || idx != selected_index.get())
                    >
                        {self.description}
                    </span>
                </div>
                <div class="flex flex-grow"></div>
                <span
                    class=("text-(--ui-hover-color)", move || idx == selected_index.get())
                    class=("text-(--ui-base-color)", move || idx != selected_index.get())
                >
                    {self.source}
                </span>
            </button>
        }
    }

    fn match_fields() -> Vec<fn(&Self) -> Option<String>> {
        vec![|command| Some(command.name.clone()), |command| {
            Some(command.description.clone())
        }]
    }
}

#[component]
pub fn SelectionMenu(menu: RwSignal<MenuType>, input_ref: NodeRef<Div>) -> impl IntoView {
    let state: AppState = expect_context();

    let mention_matches: RwSignal<Vec<MemberProfile>> = expect_context();
    let command_matches: RwSignal<Vec<Command>> = expect_context();

    let matcher = StoredValue::new(Matcher::new(Config::DEFAULT));

    let members_resource = LocalResource::new(move || {
        let room_id = state.active_room_id();
        async move {
            if let Some(rid) = room_id {
                let mut res = get_members_for_room(&rid).await.unwrap_or_default();
                res.insert(0, room_as_profile(rid));
                res.sort_by_key(|a| a.get_name());

                res
            } else {
                Vec::new()
            }
        }
    });

    let commands_resource = LocalResource::new(move || async move {
        get_commands()
            .await
            .map_err(|e| error!("Failed to get commands: {e}"))
            .unwrap_or_default()
    });

    Effect::new(move |_| {
        let mut m = matcher.get_value();

        match menu.get() {
            MenuType::UserAutocomplete { filter, .. } => {
                mention_matches.set(filter_items(
                    filter,
                    members_resource.get().unwrap_or_default(),
                    &mut m,
                ));
            }
            MenuType::CommandAutocomplete { filter, .. } => {
                command_matches.set(filter_items(
                    filter,
                    commands_resource.get().unwrap_or_default(),
                    &mut m,
                ));
            }
            MenuType::None => {
                mention_matches.set(Vec::new());
                command_matches.set(Vec::new());
            }
        }
    });

    let title_text = move || match menu.get() {
        MenuType::UserAutocomplete { filter, .. } => {
            let len = mention_matches.get().len();
            if filter.is_empty() {
                format!("MEMBERS ({len})")
            } else {
                format!("MEMBERS MATCHING @{filter} ({len})")
            }
        }
        MenuType::CommandAutocomplete { filter, .. } => {
            let len = command_matches.get().len();
            if filter.is_empty() {
                format!("COMMANDS ({len})")
            } else {
                format!("COMMANDS MATCHING /{filter} ({len})")
            }
        }
        MenuType::None => String::new(),
    };

    let content = move || {
        let Some(el) = input_ref.get() else {
            return ().into_any();
        };
        let room_id = state.active_room_id().unwrap_or_default();
        let room_id_command = room_id.clone();
        let el_command = el.clone();

        view! {
            <span class="text-(--ui-base-color) bold text-xs p-2 bb-4">{title_text()}</span>
            <For
                each=move || mention_matches.get().into_iter().enumerate()
                key=|(_, member)| member.id()
                children=move |(idx, member)| {
                    let room_id = room_id.clone();
                    let el = el.clone();
                    member.render_row(room_id, el, idx)
                }
            />
            <For
                each=move || command_matches.get().into_iter().enumerate()
                key=|(_, command)| command.id()
                children=move |(idx, command)| {
                    let room_id = room_id_command.clone();
                    let el = el_command.clone();
                    command.render_row(room_id, el, idx)
                }
            />
        }
        .into_any()
    };

    view! {
        <div
            class="mb-(--gap) absolute bottom-full left-4 right-4 bottom-(--gap) bg-(--ui-floating-hover-bg) backdrop-blur-2xl rounded-(--ui-border-radius) border border-(--tile-border-color) flex flex-col text-xs pb-(--gap) max-h-100 overflow-y-auto"
            class:hidden=move || menu.get().is_none()
        >
            {move || content}
        </div>
    }.into_any()
}
