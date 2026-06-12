use crate::{
    components::{
        input::{get_caret_position, get_node_and_offset},
        presence::PresenceBadge,
        text::RichTextExt,
        user_profile::{MemberProfileExt, RoomProfileExt},
        CloseButton,
    },
    state::{AppState, ProfileStore},
    tauri_functions::get_commands,
};
use leptos::html::{Button, Div};
use leptos::prelude::*;
use log::error;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use shared::{
    commands::Command,
    profile::{MemberProfile, RoomProfile},
    timeline::{RichTextSpan, RoomIdFormat},
};
use web_sys::{
    Document, HtmlDivElement, HtmlElement, Node, Range, ScrollBehavior, ScrollIntoViewOptions,
    ScrollLogicalPosition, ScrollToOptions,
};

#[derive(Clone, PartialEq)]
pub struct EmojiItem {
    name: String,
    shortcodes: Vec<String>,
    character: String,
}

#[derive(Clone, PartialEq, Debug)]
pub enum MenuType {
    None,
    UserAutocomplete { filter: String },
    CommandAutocomplete { filter: String },
    RoomAutocomplete { filter: String },
    EmojiAutocomplete { filter: String },
}

impl MenuType {
    fn is_none(&self) -> bool {
        matches!(self, MenuType::None)
    }
}

#[derive(Clone)]
pub enum SelectedItem {
    User(MemberProfile),
    Command(Command),
    Room(RoomProfile),
    Emoji(EmojiItem),
}

impl SelectedItem {
    fn id(&self) -> String {
        match self {
            SelectedItem::User(profile) => profile.id(),
            SelectedItem::Command(command) => command.id(),
            SelectedItem::Room(room) => room.id(),
            SelectedItem::Emoji(emoji) => emoji.id(),
        }
    }

    fn render_row(
        self,
        room_id: String,
        idx: usize,
        selected_index: RwSignal<usize>,
    ) -> impl IntoView {
        match self {
            SelectedItem::User(user) => user.render_row(room_id, idx, selected_index).into_any(),
            SelectedItem::Command(command) => {
                command.render_row(room_id, idx, selected_index).into_any()
            }
            SelectedItem::Room(room) => room.render_row(room_id, idx, selected_index).into_any(),
            SelectedItem::Emoji(emoji) => emoji.render_row(room_id, idx, selected_index).into_any(),
        }
    }
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

impl From<RoomProfile> for SelectedItem {
    fn from(profile: RoomProfile) -> Self {
        SelectedItem::Room(profile)
    }
}

impl From<EmojiItem> for SelectedItem {
    fn from(emoji: EmojiItem) -> Self {
        SelectedItem::Emoji(emoji)
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

            let mention_view = if membership.is_room() {
                let name = state
                    .active_room_name_untracked()
                    .unwrap_or(room_id.clone());

                RichTextSpan::RoomMention {
                    room_id: RoomIdFormat::Id(room_id.clone()),
                    display_name: format!("#{}", name),
                }
                .render(store, room_id)
                .into_any()
            } else {
                membership.to_span().render(store, room_id).into_any()
            };

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
        SelectedItem::Room(room) => {
            let current_room_id = state.active_room_id_untracked().unwrap_or_default();

            let room_view = room.to_span().render(store, current_room_id).into_any();

            let any_view: AnyView = room_view.into_any();
            let mut render_state = any_view.build();

            let temp_container = doc.create_element("div").unwrap();
            render_state.mount(&temp_container, None);

            let room_node = temp_container
                .first_child()
                .expect("Room view should have at least one root element");

            let space_node = doc.create_text_node("\u{00A0}");

            range.insert_node(&space_node).unwrap();
            range.insert_node(&room_node).unwrap();

            (web_sys::Node::from(space_node), 1)
        }
        // Insert emojis like macro commands
        SelectedItem::Emoji(emoji) => {
            let text_node = doc.create_text_node(&emoji.character);
            let text_len = text_node.length();
            range.insert_node(&text_node).unwrap();
            (Node::from(text_node), text_len)
        }
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
        return items.into_iter().take(50).collect();
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
    matched.into_iter().take(50).map(|(_, item)| item).collect()
}

fn scroll_into_view_when_selected(
    row_ref: NodeRef<Button>,
    idx: usize,
    selected_index: RwSignal<usize>,
) {
    Effect::new(move |_| {
        if selected_index.get() != idx {
            return;
        }
        let Some(el) = row_ref.get() else {
            return;
        };

        // scroll_into_view aligns the row's border box with the container, ignoring the
        // `*:first:mt-1` margin above the first row, so it stops short of the true top.
        if idx == 0 {
            if let Ok(Some(container)) = el.closest(".overflow-y-auto") {
                let options = ScrollToOptions::new();
                options.set_top(0.0);
                options.set_behavior(ScrollBehavior::Smooth);
                container.scroll_to_with_scroll_to_options(&options);
            }
            return;
        }

        let options = ScrollIntoViewOptions::new();
        options.set_behavior(ScrollBehavior::Smooth);
        options.set_block(ScrollLogicalPosition::Nearest);
        el.scroll_into_view_with_scroll_into_view_options(&options);
    });
}

trait RenderMenuRow {
    fn id(&self) -> String;
    fn render_row(
        self,
        room_id: String,
        idx: usize,
        selected_index: RwSignal<usize>,
    ) -> impl IntoView;
    fn match_fields() -> Vec<fn(&Self) -> Option<String>>;
}

impl RenderMenuRow for MemberProfile {
    fn id(&self) -> String {
        self.user_id().to_string()
    }

    fn render_row(
        self,
        room_id: String,
        idx: usize,
        selected_index: RwSignal<usize>,
    ) -> impl IntoView {
        let state: AppState = expect_context();
        let store: ProfileStore = expect_context();

        let presence = store.get_presence(self.user_id());
        let profile = store.get_member_profile(&room_id, self.user_id()).get();
        let p_clone = profile.clone();
        let m_clone = self.clone();

        view! {
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

    fn render_row(self, _: String, idx: usize, selected_index: RwSignal<usize>) -> impl IntoView {
        view! {
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
        }
    }

    fn match_fields() -> Vec<fn(&Self) -> Option<String>> {
        vec![|command| Some(command.name.clone()), |command| {
            Some(command.description.clone())
        }]
    }
}

impl RenderMenuRow for RoomProfile {
    fn id(&self) -> String {
        self.room_id.clone()
    }

    fn render_row(self, _: String, idx: usize, selected_index: RwSignal<usize>) -> impl IntoView {
        view! {
            <span class="text-start text-sm text-normal">{self.get_name()}</span>
            <div class="flex flex-grow"></div>
            <span
                class="text-start"
                class=("text-(--ui-hover-color)", move || idx == selected_index.get())
                class=("text-(--ui-base-color)", move || idx != selected_index.get())
            >
                {self.canonical_alias.clone().unwrap_or(self.room_id.clone())}
            </span>
        }
    }

    fn match_fields() -> Vec<fn(&Self) -> Option<String>> {
        vec![
            |room| room.name.clone(),
            |room| room.canonical_alias.clone(),
            |room| {
                if room.aliases.is_empty() {
                    None
                } else {
                    Some(room.aliases.join(" "))
                }
            },
        ]
    }
}

impl RenderMenuRow for EmojiItem {
    fn id(&self) -> String {
        self.name.clone()
    }

    fn render_row(self, _: String, idx: usize, selected_index: RwSignal<usize>) -> impl IntoView {
        view! {
            <span class="text-xl">{self.character}</span>
            <span class="text-start text-sm text-normal">{self.name}</span>
            <div class="flex flex-grow"></div>
            <span
                class=("text-(--ui-hover-color)", move || idx == selected_index.get())
                class=("text-(--ui-base-color)", move || idx != selected_index.get())
            >
                {self.shortcodes.join(", ")}
            </span>
        }
    }

    fn match_fields() -> Vec<fn(&Self) -> Option<String>> {
        vec![
            |emoji| {
                if emoji.shortcodes.is_empty() {
                    None
                } else {
                    Some(emoji.shortcodes.join(" "))
                }
            },
            |emoji| Some(emoji.name.clone()),
        ]
    }
}

#[derive(Clone)]
pub enum MenuCompletionMatches {
    User(Vec<MemberProfile>),
    Command(Vec<Command>),
    Room(Vec<RoomProfile>),
    Emoji(Vec<EmojiItem>),
    None,
}

impl MenuCompletionMatches {
    pub fn len(&self) -> usize {
        match self {
            MenuCompletionMatches::User(members) => members.len(),
            MenuCompletionMatches::Command(commands) => commands.len(),
            MenuCompletionMatches::Room(rooms) => rooms.len(),
            MenuCompletionMatches::Emoji(emojis) => emojis.len(),
            MenuCompletionMatches::None => 0,
        }
    }

    fn to_vec(&self) -> Vec<SelectedItem> {
        match self {
            MenuCompletionMatches::User(members) if !members.is_empty() => members
                .clone()
                .into_iter()
                .map(SelectedItem::from)
                .collect(),
            MenuCompletionMatches::Command(commands) if !commands.is_empty() => commands
                .clone()
                .into_iter()
                .map(SelectedItem::from)
                .collect(),
            MenuCompletionMatches::Room(rooms) if !rooms.is_empty() => {
                rooms.clone().into_iter().map(SelectedItem::from).collect()
            }
            MenuCompletionMatches::Emoji(emojis) if !emojis.is_empty() => {
                emojis.clone().into_iter().map(SelectedItem::from).collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn get(&self, index: usize) -> Option<SelectedItem> {
        match self {
            MenuCompletionMatches::User(members) => {
                members.get(index).cloned().map(SelectedItem::from)
            }
            MenuCompletionMatches::Command(commands) => {
                commands.get(index).cloned().map(SelectedItem::from)
            }
            MenuCompletionMatches::Room(rooms) => rooms.get(index).cloned().map(SelectedItem::from),
            MenuCompletionMatches::Emoji(emojis) => {
                emojis.get(index).cloned().map(SelectedItem::from)
            }
            MenuCompletionMatches::None => None,
        }
    }
}

fn auto_complete_item_render(
    el: HtmlDivElement,
    room_id: String,
    selected_index: RwSignal<usize>,
    idx: usize,
    item: SelectedItem,
) -> impl IntoView {
    let content = item.clone().render_row(room_id, idx, selected_index);

    let row_ref: NodeRef<Button> = NodeRef::new();
    scroll_into_view_when_selected(row_ref, idx, selected_index);

    let el = el.clone();
    view! {
        <button
            node_ref=row_ref
            class="flex flex-row justify-center items-center rounded-(--ui-border-radius) cursor-pointer mx-(--gap) px-(--gap) py-1"
            class=("bg-(--ui-hover-bg)", move || selected_index.get() == idx)
            on:mouseenter=move |_| selected_index.set(idx)
            on:click=move |_| {
                commit_selection(&el, item.clone(), expect_context(), expect_context())
            }
        >
            {content}
        </button>
    }
}

#[component]
pub fn SelectionMenu(menu: RwSignal<MenuType>, input_ref: NodeRef<Div>) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let matches: RwSignal<MenuCompletionMatches> = expect_context();

    let matcher = StoredValue::new(Matcher::new(Config::DEFAULT));

    let members_resource = Memo::new(move |_| {
        let room_id = state.active_room_id().unwrap_or_default();
        store.clone().get_members(&room_id)
    });

    let commands_resource = LocalResource::new(move || async move {
        get_commands()
            .await
            .map_err(|e| error!("Failed to get commands: {e}"))
            .unwrap_or_default()
    });

    let room_resource = Memo::new(move |_| state.get_room_profiles_in_active_server());

    let emoji_resource: Memo<Vec<EmojiItem>> = Memo::new(move |_| {
        emojis::iter()
            .map(|emoji| EmojiItem {
                name: emoji.name().to_string(),
                shortcodes: emoji.shortcodes().map(|s| s.to_string()).collect(),
                character: emoji.as_str().to_string(),
            })
            .collect()
    });

    Effect::new(move |_| {
        let mut m = matcher.get_value();

        let new_matches = match menu.get() {
            MenuType::UserAutocomplete { filter, .. } => {
                MenuCompletionMatches::User(filter_items(filter, members_resource.get(), &mut m))
            }
            MenuType::CommandAutocomplete { filter, .. } => MenuCompletionMatches::Command(
                filter_items(filter, commands_resource.get().unwrap_or_default(), &mut m),
            ),
            MenuType::RoomAutocomplete { filter } => {
                MenuCompletionMatches::Room(filter_items(filter, room_resource.get(), &mut m))
            }
            MenuType::EmojiAutocomplete { filter } => {
                MenuCompletionMatches::Emoji(filter_items(filter, emoji_resource.get(), &mut m))
            }
            MenuType::None => MenuCompletionMatches::None,
        };

        matches.set(new_matches);
    });

    let title_text = move || {
        let len = matches.get().len();
        match menu.get() {
            MenuType::UserAutocomplete { filter, .. } => {
                if filter.is_empty() {
                    format!("MEMBERS ({len})")
                } else {
                    format!("MEMBERS MATCHING @{filter} ({len})")
                }
            }
            MenuType::CommandAutocomplete { filter, .. } => {
                if filter.is_empty() {
                    format!("COMMANDS ({len})")
                } else {
                    format!("COMMANDS MATCHING /{filter} ({len})")
                }
            }
            MenuType::RoomAutocomplete { filter, .. } => {
                if filter.is_empty() {
                    format!("ROOMS ({len})")
                } else {
                    format!("ROOMS MATCHING #{filter} ({len})")
                }
            }
            MenuType::EmojiAutocomplete { filter, .. } => {
                if filter.is_empty() {
                    format!("EMOJIS ({len})")
                } else {
                    format!("EMOJIS MATCHING :{filter}: ({len})")
                }
            }
            MenuType::None => String::new(),
        }
    };

    let no_matches = move || matches.get().len() == 0;

    let selected_index: RwSignal<usize> = expect_context();

    let content = move || {
        let Some(el) = input_ref.get() else {
            return ().into_any();
        };
        let room_id = state.active_room_id().unwrap_or_default();

        view! {
            <span class="text-(--ui-base-color) bold text-xs p-2 bb-4 border-b border-(--tile-border-color)">
                {title_text()}
            </span>
            <div class="overflow-y-auto *:first:mt-1 flex flex-col">
                <For
                    each=move || matches.get().to_vec().into_iter().take(50).enumerate()
                    key=|(_, row)| row.id()
                    children=move |(idx, row)| {
                        let room_id = room_id.clone();
                        let el = el.clone();
                        auto_complete_item_render(el, room_id, selected_index, idx, row)
                    }
                />
            </div>
        }
        .into_any()
    };

    view! {
        <div
            class="mb-(--gap) absolute bottom-full left-4 right-4 bottom-(--gap) bg-(--ui-floating-hover-bg) backdrop-blur-2xl rounded-(--ui-border-radius) border border-(--tile-border-color) flex flex-col text-xs pb-(--gap) max-h-100"
            class:hidden=move || menu.get().is_none() || no_matches()
        >
            <CloseButton on_click=move |_| menu.set(MenuType::None) inset="3px" />
            {content}
        </div>
    }.into_any()
}
